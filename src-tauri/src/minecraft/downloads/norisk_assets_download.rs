use crate::config::{ProjectDirsExt, HTTP_CLIENT, LAUNCHER_DIRECTORY};
use crate::error::{AppError, Result};
use crate::minecraft::api::NoRiskApi;
use crate::minecraft::auth::minecraft_auth::Credentials;
use crate::minecraft::dto::norisk_meta::NoriskAssets;
use crate::minecraft::dto::piston_meta::AssetObject;
use crate::state::event_state::{EventPayload, EventType};
use crate::state::profile_state::Profile;
use crate::state::State;
use futures::stream::{iter, StreamExt};
use log::{debug, error, info, trace, warn};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

const ASSETS_DIR: &str = "assets";
const NORISK_ASSETS_DIR: &str = "noriskclient";
const DEFAULT_CONCURRENT_DOWNLOADS: usize = 12;

pub struct NoriskClientAssetsDownloadService {
    base_path: PathBuf,
    concurrent_downloads: usize,
}

impl NoriskClientAssetsDownloadService {
    pub fn new() -> Self {
        let base_path = LAUNCHER_DIRECTORY.meta_dir().join(ASSETS_DIR);
        info!(
            "[NRC Assets Service] Initialized. Base Path: {}",
            base_path.display()
        );
        Self {
            base_path,
            concurrent_downloads: DEFAULT_CONCURRENT_DOWNLOADS,
        }
    }

    /// Sets the number of concurrent downloads to use
    pub fn with_concurrent_downloads(mut self, concurrent_downloads: usize) -> Self {
        self.concurrent_downloads = concurrent_downloads;
        self
    }

    /// Downloads NoRisk client assets for a specific profile, processing the main pack
    /// and any additional asset groups defined in the pack configuration.
    pub async fn download_nrc_assets_for_profile(
        &self,
        profile: &Profile,
        credentials: Option<&Credentials>,
        is_experimental: bool,
    ) -> Result<()> {
        let state = State::get().await?;
        let game_directory = state
            .profile_manager
            .calculate_instance_path_for_profile(profile)?;

        let keep_local_assets = profile
            .norisk_information
            .as_ref()
            .map(|info| info.keep_local_assets)
            .unwrap_or(false);

        if keep_local_assets {
            info!("[NRC Assets Download] Keep local assets flag is enabled for this profile");
        }

        // Use profile's selected pack as the *main* pack ID
        let main_pack_id = match &profile.selected_norisk_pack_id {
            Some(pack_id) if !pack_id.is_empty() => {
                info!(
                    "[NRC Assets Download] Using main pack ID from profile: {}",
                    pack_id
                );
                pack_id.clone()
            }
            _ => {
                info!(
                    "[NRC Assets Download] No pack specified in profile, skipping asset download"
                );
                return Ok(());
            }
        };

        let creds = match credentials {
            Some(c) => c,
            None => {
                warn!("[NRC Assets Download] No credentials provided, skipping asset download");
                return Ok(());
            }
        };

        let token_ref = if is_experimental {
            info!("[NRC Assets Download] Using experimental token");
            &creds.norisk_credentials.experimental
        } else {
            info!("[NRC Assets Download] Using production token");
            &creds.norisk_credentials.production
        };

        let norisk_token = match token_ref {
            Some(token) => token.value.clone(),
            None => {
                warn!("[NRC Assets Download] No valid NoRisk token found for {} mode, skipping asset download",
                      if is_experimental { "experimental" } else { "production" });
                return Ok(());
            }
        };

        let request_uuid = creds.id.to_string();
        info!(
            "[NRC Assets Download] Using request UUID from credentials: {}",
            request_uuid
        );

        // --- Get Resolved Pack Definition ---
        info!("[NRC Assets Download] Getting Norisk packs config...");
        let norisk_packs_config = state.norisk_pack_manager.get_config().await;

        info!(
            "[NRC Assets Download] Resolving pack definition for main pack: {}",
            main_pack_id
        );
        let resolved_pack_definition = norisk_packs_config
            .get_resolved_pack_definition(&main_pack_id)
            .map_err(|e| {
                error!(
                    "Failed to get resolved pack definition for '{}': {}",
                    main_pack_id, e
                );
                AppError::Other(format!(
                    "Failed to resolve pack definition {}: {}",
                    main_pack_id, e
                ))
            })?;

        // --- Collect All Asset Groups to Process ---
        let mut asset_ids_to_process = vec![];
        asset_ids_to_process.extend(resolved_pack_definition.assets.iter().cloned());
        let unique_asset_ids: Vec<String> = {
            let mut seen = HashSet::new();
            asset_ids_to_process
                .into_iter()
                .filter(|id| seen.insert(id.clone()))
                .collect()
        };

        info!(
            "[NRC Assets Download] Identified asset groups to process: {:?}",
            unique_asset_ids
        );
        let total_groups = unique_asset_ids.len();
        let target_base_dir = game_directory.join("NoRiskClient").join("assets");

        self.emit_progress_event(
            &state,
            profile.id,
            &format!(
                "Starting NoRiskClient asset processing for {} groups...",
                total_groups
            ),
            0.01,
            None,
        )
        .await?;

        // --- Process Each Asset Group ---
        let mut all_expected_target_paths: HashSet<PathBuf> = HashSet::new();

        for (index, asset_id) in unique_asset_ids.iter().enumerate() {
            let group_progress_start = (index as f64 / total_groups as f64) * 0.9 + 0.01; // Scale 0.01 to 0.91
            let group_progress_end = ((index + 1) as f64 / total_groups as f64) * 0.9 + 0.01;

            info!(
                "--- Processing Asset Group {}/{} ('{}') ---",
                index + 1,
                total_groups,
                asset_id
            );

            match self
                .process_asset_group(
                    &state,
                    profile.id,
                    asset_id,
                    &norisk_token,
                    &request_uuid,
                    is_experimental,
                    keep_local_assets,
                    &game_directory,
                    group_progress_start,
                    group_progress_end,
                )
                .await
            {
                Ok(expected_paths_for_group) => {
                    info!(
                        "--- Successfully finished processing asset group '{}' ({} expected paths) ---",
                         asset_id, expected_paths_for_group.len()
                    );
                    // Merge the expected paths from this group
                    all_expected_target_paths.extend(expected_paths_for_group);
                }
                Err(e) => {
                    error!(
                        "--- Error processing asset group '{}': {}. Continuing... ---",
                        asset_id, e
                    );
                    self.emit_progress_event(
                        &state,
                        profile.id,
                        &format!("Error processing asset group: {}", asset_id),
                        group_progress_end,
                        Some(e.to_string()),
                    )
                    .await?;
                }
            }
        }

        // --- Cleanup Orphan Assets (only if keep_local_assets is false) ---
        if !keep_local_assets {
            info!(
                "[NRC Assets Cleanup] Cleaning up orphan files in target directory: {}",
                target_base_dir.display()
            );
            self.emit_progress_event(
                &state,
                profile.id,
                "Cleaning up old asset files...",
                0.95, // Progress for cleanup phase
                None,
            )
            .await?;

            match self
                .cleanup_orphan_assets(&target_base_dir, &all_expected_target_paths)
                .await
            {
                Ok(deleted_count) => info!(
                    "[NRC Assets Cleanup] Successfully cleaned up {} orphan items.",
                    deleted_count
                ),
                Err(e) => {
                    error!("[NRC Assets Cleanup] Failed during cleanup: {}", e);
                    // Emit cleanup error event, but proceed to final completion event
                    self.emit_progress_event(
                        &state,
                        profile.id,
                        "Error during asset cleanup",
                        0.98, // Mark cleanup error progress
                        Some(e.to_string()),
                    )
                    .await?;
                }
            }
        } else {
            info!("[NRC Assets Cleanup] Skipping cleanup because keep_local_assets is enabled.");
        }

        // --- Final Progress Update ---
        info!("[NRC Assets Download] All asset groups processed and cleanup attempted.");
        self.emit_progress_event(
            &state,
            profile.id,
            "NoRiskClient assets processing completed!",
            1.0,
            None,
        )
        .await?;

        Ok(())
    }

    /// Processes a single asset group: Fetches metadata, downloads assets, copies to game dir.
    /// Returns the set of expected target paths for cleanup.
    async fn process_asset_group(
        &self,
        state: &State,
        profile_id: Uuid,
        asset_id: &str,
        norisk_token: &str,
        request_uuid: &str,
        is_experimental: bool,
        keep_local_assets: bool,
        game_directory: &PathBuf,
        progress_start: f64,
        progress_end: f64,
    ) -> Result<HashSet<PathBuf>> {
        let progress_range = progress_end - progress_start;
        let target_base_dir = game_directory.join("NoRiskClient").join("assets");

        // 1. Fetch assets
        self.emit_progress_event(
            state,
            profile_id,
            &format!("Fetching assets for group: {}...", asset_id),
            progress_start + progress_range * 0.05,
            None,
        )
        .await?;

        let assets =
            match NoRiskApi::norisk_assets(asset_id, norisk_token, request_uuid, is_experimental)
                .await
            {
                Ok(fetched_assets) => {
                    info!(
                        "[NRC Assets Group '{}'] Assets fetched successfully. Found {} objects.",
                        asset_id,
                        fetched_assets.objects.len()
                    );
                    if fetched_assets.objects.is_empty() {
                        warn!(
                            "[NRC Assets Group '{}'] No assets found. Skipping download/copy.",
                            asset_id
                        );
                        self.emit_progress_event(
                            state,
                            profile_id,
                            &format!("No assets found for group: {}", asset_id),
                            progress_end,
                            None,
                        )
                        .await?;
                        // Return empty set as no paths are expected
                        return Ok(HashSet::new());
                    }
                    if let Some((key, obj)) = fetched_assets.objects.iter().next() {
                        debug!(
                            "[NRC Assets Group '{}'] Sample asset - Key: {}, Hash: {}, Size: {}",
                            asset_id, key, obj.hash, obj.size
                        );
                    }
                    fetched_assets
                }
                Err(e) => {
                    error!(
                        "[NRC Assets Group '{}'] Failed to fetch assets: {}. Skipping.",
                        asset_id, e
                    );
                    self.emit_progress_event(
                        state,
                        profile_id,
                        &format!("Failed to fetch assets for group: {}", asset_id),
                        progress_start + progress_range * 0.1,
                        Some(e.to_string()),
                    )
                    .await?;
                    return Err(AppError::Download(format!(
                        "Failed to fetch assets for {}: {}",
                        asset_id, e
                    )));
                }
            };

        // --- Calculate Expected Paths Before Download/Copy ---
        let mut expected_paths_for_group: HashSet<PathBuf> = HashSet::new();
        for name in assets.objects.keys() {
            let target_path = target_base_dir.join(name);
            // Add the file path itself
            expected_paths_for_group.insert(target_path.clone());
            // Add all parent directories recursively up to the target base directory
            let mut current_parent = target_path.parent();
            while let Some(parent) = current_parent {
                if parent == target_base_dir
                    || parent.starts_with(&target_base_dir)
                        && parent.components().count() > target_base_dir.components().count()
                {
                    if expected_paths_for_group.insert(parent.to_path_buf()) {
                        current_parent = parent.parent(); // Continue upwards only if path was newly inserted
                    } else {
                        break; // Stop if parent was already added (avoids redundant checks)
                    }
                } else {
                    break; // Stop if we reached or went above the target base dir
                }
            }
        }
        // Add the base target directory itself if it exists or needs to be created
        expected_paths_for_group.insert(target_base_dir.clone());

        // 2. Download assets
        self.emit_progress_event(
            state,
            profile_id,
            &format!(
                "Downloading assets for group: {} ({} files)...",
                asset_id,
                assets.objects.len()
            ),
            progress_start + progress_range * 0.1,
            None,
        )
        .await?;

        match self
            .download_nrc_assets(
                asset_id,
                &assets,
                is_experimental,
                norisk_token,
                Some(profile_id),
            )
            .await
        {
            Ok(_) => info!(
                "[NRC Assets Group '{}'] Successfully downloaded assets.",
                asset_id
            ),
            Err(e) => {
                error!(
                    "[NRC Assets Group '{}'] Failed to download assets: {}. Skipping copy.",
                    asset_id, e
                );
                return Err(e);
            }
        }

        // 3. Copy assets
        self.emit_progress_event(
            state,
            profile_id,
            &format!("Copying assets for group: {}...", asset_id),
            progress_start + progress_range * 0.9,
            None,
        )
        .await?;

        // Pass target_base_dir to copy function
        match self
            .copy_assets_to_game_dir(
                asset_id,
                &assets,
                keep_local_assets,
                &target_base_dir,
                game_directory,
                Some(profile_id),
            )
            .await
        {
            Ok(_) => info!(
                "[NRC Assets Group '{}'] Successfully copied assets.",
                asset_id
            ),
            Err(e) => {
                error!(
                    "[NRC Assets Group '{}'] Failed to copy assets: {}. Skipping cleanup for this group's contribution.",
                    asset_id, e
                );
                // Return error, main loop will handle it, but DON'T return the expected paths set
                // as the copy failed. Or maybe return empty set? Let's return error.
                return Err(e);
            }
        }

        self.emit_progress_event(
            state,
            profile_id,
            &format!("Finished processing asset group: {}", asset_id),
            progress_end,
            None,
        )
        .await?;

        // Return the calculated expected paths for this group
        Ok(expected_paths_for_group)
    }

    /// Downloads NoRisk client assets for a specific asset ID (pack or asset group).
    async fn download_nrc_assets(
        &self,
        asset_id: &str,
        assets: &NoriskAssets,
        is_experimental: bool,
        norisk_token: &str,
        profile_id: Option<Uuid>,
    ) -> Result<()> {
        trace!(
            "[NRC Assets Download '{}'] Starting download process",
            asset_id
        );
        let assets_path = self.base_path.join(NORISK_ASSETS_DIR).join(asset_id);
        if !fs::try_exists(&assets_path).await? {
            fs::create_dir_all(&assets_path).await?;
            info!(
                "[NRC Assets Download '{}'] Created directory: {}",
                asset_id,
                assets_path.display()
            );
        }

        let assets_list: Vec<(String, AssetObject)> = assets
            .objects
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let mut downloads = Vec::new();
        let task_counter = Arc::new(AtomicUsize::new(1));
        let completed_counter = Arc::new(AtomicUsize::new(0));
        let total_to_download = Arc::new(AtomicUsize::new(0));

        trace!(
            "[NRC Assets Download '{}'] Preparing {} potential jobs...",
            asset_id,
            assets_list.len()
        );
        let mut job_count = 0;

        let state = if profile_id.is_some() {
            State::get().await.ok() // Change to ok() to allow optional state
        } else {
            None
        };
        // Clone state before the closure so the original `state` remains available after the loop
        let state_clone_for_inspect = state.clone();

        for (name, asset) in assets_list {
            let hash = asset.hash.clone();
            let size = asset.size;
            
            // Store assets by hash like Mojang: objects/ab/abcdef123...
            let hash_prefix = &hash[0..2]; // First 2 chars
            let objects_dir = assets_path.join("objects").join(hash_prefix);
            let target_path = objects_dir.join(&hash);
            
            let name_clone = name.clone();
            let task_counter_clone = Arc::clone(&task_counter);
            let completed_counter_clone = Arc::clone(&completed_counter);
            let total_to_download_clone = Arc::clone(&total_to_download);
            let asset_id_clone = asset_id.to_string();
            let norisk_token_clone = norisk_token.to_string();

            // Hash-based check - if hash file exists, content is guaranteed correct
            if fs::try_exists(&target_path).await? {
                trace!(
                    "[NRC Assets Download '{}'] Skipping asset {} (hash {} already exists)",
                    asset_id_clone,
                    name_clone,
                    hash
                );
                continue;
            }

            job_count += 1;
            total_to_download_clone.fetch_add(1, Ordering::SeqCst);
            downloads.push(async move {
                let task_id = task_counter_clone.fetch_add(1, Ordering::SeqCst);
                trace!("[NRC Assets Download '{}' Task {}] Starting download for: {}", asset_id_clone, task_id, name_clone);

                // Use updated URL format from user edit
                let url = format!(
                    "{}/{}/assets/{}",
                    "https://cdn.norisk.gg/assets", asset_id_clone, name_clone
                );

                let mut request = HTTP_CLIENT.get(&url);
                request = request.header("Authorization", format!("Bearer {}", norisk_token_clone));

                let response = match request.send().await {
                    Ok(resp) => resp,
                    Err(e) => {
                        error!("[NRC Assets Download '{}' Task {}] Request error for {}: {}", asset_id_clone, task_id, name_clone, e);
                        return Err(AppError::Download(format!("Request failed for {}: {}", name_clone, e)));
                    }
                };

                if !response.status().is_success() {
                    let status = response.status();
                    let error_text = response.text().await.unwrap_or_else(|_| "No error details".to_string());
                    error!("[NRC Assets Download '{}' Task {}] Failed download for {}: Status {}, Error: {}",
                           asset_id_clone, task_id, name_clone, status, error_text);
                    return Err(AppError::Download(format!("Download failed for {} - Status {}: {}", name_clone, status, error_text)));
                }

                let bytes = match response.bytes().await {
                    Ok(b) => b,
                    Err(e) => {
                         error!("[NRC Assets Download '{}' Task {}] Error reading bytes for {}: {}", asset_id_clone, task_id, name_clone, e);
                         return Err(AppError::Download(format!("Read bytes failed for {}: {}", name_clone, e)));
                    }
                };

                if let Some(parent) = target_path.parent() {
                    if let Err(e) = fs::create_dir_all(parent).await {
                        error!("[NRC Assets Download '{}' Task {}] Error creating dir for {}: {}", asset_id_clone, task_id, name_clone, e);
                         return Err(AppError::Io(e));
                    }
                }

                let mut file = match fs::File::create(&target_path).await {
                    Ok(f) => f,
                    Err(e) => {
                        error!("[NRC Assets Download '{}' Task {}] Error creating file for {}: {}", asset_id_clone, task_id, name_clone, e);
                         return Err(AppError::Io(e));
                    }
                };

                if let Err(e) = file.write_all(&bytes).await {
                    error!("[NRC Assets Download '{}' Task {}] Error writing file for {}: {}", asset_id_clone, task_id, name_clone, e);
                     return Err(AppError::Io(e));
                }

                let completed = completed_counter_clone.fetch_add(1, Ordering::SeqCst) + 1;
                let total = total_to_download_clone.load(Ordering::SeqCst);

                info!("[NRC Assets Download '{}' Task {}] Finished download for: {} ({}/{})",
                      asset_id_clone, task_id, name_clone, completed, total);
                Ok(())
            });
        }

        info!(
            "[NRC Assets Download '{}'] Queued {} actual download tasks.",
            asset_id, job_count
        );

        if job_count == 0 {
            info!(
                "[NRC Assets Download '{}'] No new assets to download.",
                asset_id
            );
            if let (Some(state_ref), Some(profile_id_val)) = (&state, profile_id) {
                self.emit_progress_event(
                    state_ref,
                    profile_id_val,
                    &format!("Assets for group '{}' are up to date!", asset_id),
                    0.8,
                    None,
                )
                .await?;
            }
            return Ok(());
        }

        info!(
            "[NRC Assets Download '{}'] Processing tasks with {} concurrent downloads...",
            asset_id, self.concurrent_downloads
        );

        let completed_ref = Arc::clone(&completed_counter);
        let asset_id_clone = asset_id.to_string();

        let results: Vec<Result<()>> = iter(downloads)
            .buffer_unordered(self.concurrent_downloads)
            .inspect({
                let asset_id_inspect = asset_id_clone.clone();
                 // Use the cloned state inside the closure
                move |_| {
                    if let (Some(state_ref), Some(profile_id_val)) = (&state_clone_for_inspect, profile_id) {
                        let completed = completed_ref.load(Ordering::SeqCst);
                        let total = total_to_download.load(Ordering::SeqCst);
                        if total > 0 {
                            let progress_within_download = 0.1 + (completed as f64 / total as f64) * 0.7;
                            let asset_id_for_task = asset_id_inspect.clone();
                            tokio::spawn({
                                let state = state_ref.clone();
                                let message = format!("Downloading '{}' assets: {}/{} files", asset_id_for_task, completed, total);
                                let profile_id = profile_id_val;
                                async move {
                                    let event_id = Uuid::new_v4();
                                    if let Err(e) = state.emit_event(EventPayload {
                                        event_id,
                                        event_type: EventType::DownloadingNoRiskClientAssets,
                                        target_id: Some(profile_id),
                                        message,
                                        progress: Some(progress_within_download),
                                        error: None,
                                    }).await {
                                        error!("[NRC Assets Download '{}'] Failed to emit progress event: {}", asset_id_for_task, e);
                                    }
                                }
                            });
                        }
                    }
                }
            })
            .collect()
            .await;

        let mut errors = Vec::new();
        for result in results {
            if let Err(e) = result {
                errors.push(e);
            }
        }

        if !errors.is_empty() {
            error!(
                "[NRC Assets Download '{}'] Finished with {} errors:",
                asset_id,
                errors.len()
            );
            for error_item in &errors {
                error!("  - {}", error_item);
            }
            // Use original state here, as it wasn't moved by the inspect closure
            if let (Some(state_ref), Some(profile_id_val)) = (&state, profile_id) {
                self.emit_progress_event(
                    state_ref,
                    profile_id_val,
                    &format!(
                        "Failed download for group '{}' ({} errors)",
                        asset_id,
                        errors.len()
                    ),
                    0.8,
                    Some(errors[0].to_string()),
                )
                .await?;
            }
            Err(errors.remove(0))
        } else {
            info!(
                "[NRC Assets Download '{}'] All asset downloads completed successfully.",
                asset_id
            );
            // Use original state here
            if let (Some(state_ref), Some(profile_id_val)) = (&state, profile_id) {
                self.emit_progress_event(
                    state_ref,
                    profile_id_val,
                    &format!("Asset download completed for group '{}'", asset_id),
                    0.8,
                    None,
                )
                .await?;
            }
            Ok(())
        }
    }

    /// Helper method to emit progress events
    async fn emit_progress_event(
        &self,
        state: &State,
        profile_id: Uuid,
        message: &str,
        progress: f64,
        error: Option<String>,
    ) -> Result<Uuid> {
        let event_id = Uuid::new_v4();
        state
            .emit_event(EventPayload {
                event_id,
                event_type: EventType::DownloadingNoRiskClientAssets,
                target_id: Some(profile_id),
                message: message.to_string(),
                progress: Some(progress.clamp(0.0, 1.0)),
                error,
            })
            .await?;
        Ok(event_id)
    }

    /// Copy downloaded assets to the profile's game directory for a specific asset ID.
    async fn copy_assets_to_game_dir(
        &self,
        asset_id: &str,
        assets: &NoriskAssets,
        keep_local_assets: bool,
        target_base_dir: &Path,
        minecraft_dir: &Path,
        profile_id: Option<Uuid>,
    ) -> Result<()> {
        let source_dir = self.base_path.join(NORISK_ASSETS_DIR).join(asset_id);
        // target_base_dir is now passed in

        info!(
            "[NRC Assets Copy '{}'] Copying from {} to {}",
            asset_id,
            source_dir.display(),
            target_base_dir.display() // Log the base dir
        );

        if !fs::try_exists(&source_dir).await? {
            warn!(
                "[NRC Assets Copy '{}'] Source directory {} does not exist. Nothing to copy.",
                asset_id,
                source_dir.display()
            );
            return Ok(());
        }

        if !fs::try_exists(target_base_dir).await? {
            fs::create_dir_all(target_base_dir).await?;
            info!(
                "[NRC Assets Copy '{}'] Created target directory: {}",
                asset_id,
                target_base_dir.display()
            );
        }

        let assets_list: Vec<(String, AssetObject)> = assets
            .objects
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let mut copied_count = 0;
        let mut skipped_count = 0;
        let total_assets = assets_list.len();

        let state = State::get().await.ok();

        if let (Some(state_ref), Some(profile_id_val)) = (&state, profile_id) {
            self.emit_copy_event(
                state_ref,
                profile_id_val,
                &format!(
                    "Preparing copy for group '{}': {} files",
                    asset_id, total_assets
                ),
                0.0,
                None,
            )
            .await?;
        }

        let batch_size = 50;
        let mut batch_count = 0;
        let total_batches = (total_assets + batch_size - 1) / batch_size;

        for chunk in assets_list.chunks(batch_size) {
            batch_count += 1;
            let mut batch_copied = 0;
            let mut batch_skipped = 0;

            for (name, asset) in chunk {
                let hash = &asset.hash;
                
                // Source is now hash-based: objects/ab/abcdef123...
                let hash_prefix = &hash[0..2];
                let source_path = source_dir.join("objects").join(hash_prefix).join(hash);

                // Special handling for override assets
                let (target_path, is_override) = if name.starts_with("overrides/") {
                    // For overrides, copy to Minecraft directory and strip the "overrides/" prefix
                    let relative_path = name.strip_prefix("overrides/").unwrap_or(name);
                    (minecraft_dir.join(relative_path), true)
                } else {
                    // Normal assets go to the target base directory
                    (target_base_dir.join(&name), false)
                };

                if !fs::try_exists(&source_path).await? {
                    warn!(
                        "[NRC Assets Copy '{}'] Hash file missing: {} (for asset {})",
                        asset_id,
                        source_path.display(),
                        name
                    );
                    continue;
                }

                let needs_copy = if fs::try_exists(&target_path).await? {
                    if is_override {
                        // For override files, skip if the file already exists
                        debug!(
                            "[NRC Assets Copy '{}'] Skipping override file {} (already exists)",
                            asset_id, name
                        );
                        false
                    } else if keep_local_assets {
                        debug!(
                            "[NRC Assets Copy '{}'] Keeping local asset {} (keep_local_assets)",
                            asset_id, name
                        );
                        false
                    } else {
                        // Hash-based system: check size first, then modification time
                        let source_metadata = fs::metadata(&source_path).await?;
                        let target_metadata = fs::metadata(&target_path).await?;
                        
                        if target_metadata.len() as i64 != asset.size {
                            debug!(
                                "[NRC Assets Copy '{}'] Target size mismatch for {} (hash {}), needs copy",
                                asset_id, name, hash
                            );
                            true
                        } else {
                            // Same size - check if hash file (source) is newer than target
                            let source_modified = source_metadata.modified().unwrap_or(std::time::UNIX_EPOCH);
                            let target_modified = target_metadata.modified().unwrap_or(std::time::UNIX_EPOCH);
                            
                            if source_modified > target_modified {
                                debug!(
                                    "[NRC Assets Copy '{}'] Hash file {} is newer than target, needs copy",
                                    asset_id, name
                                );
                                true
                            } else {
                                trace!(
                                    "[NRC Assets Copy '{}'] Skipping {} (hash {}), target is up to date",
                                    asset_id, name, hash
                                );
                                false
                            }
                        }
                    }
                } else {
                    if is_override {
                        debug!(
                            "[NRC Assets Copy '{}'] Override file doesn't exist for {} (hash {}), copying to Minecraft dir",
                            asset_id, name, hash
                        );
                    } else {
                        debug!(
                            "[NRC Assets Copy '{}'] Target doesn't exist for {} (hash {}), needs copy",
                            asset_id, name, hash
                        );
                    }
                    true
                };

                if needs_copy {
                    if let Some(parent) = target_path.parent() {
                        if !fs::try_exists(parent).await? {
                            fs::create_dir_all(parent).await?;
                        }
                    }
                    fs::copy(&source_path, &target_path).await?;
                    copied_count += 1;
                    batch_copied += 1;
                } else {
                    skipped_count += 1;
                    batch_skipped += 1;
                }
            }

            if let (Some(state_ref), Some(profile_id_val)) = (&state, profile_id) {
                let progress_within_copy = (batch_count as f64 / total_batches as f64) * 0.9 + 0.1;
                self.emit_copy_event(
                    state_ref,
                    profile_id_val,
                    &format!(
                        "Copying '{}' assets: Batch {}/{} (Copied: {}, Skipped: {})",
                        asset_id, batch_count, total_batches, copied_count, skipped_count
                    ),
                    progress_within_copy,
                    None,
                )
                .await?;
            }
        }

        if let (Some(state_ref), Some(profile_id_val)) = (&state, profile_id) {
            self.emit_copy_event(
                state_ref,
                profile_id_val,
                &format!(
                    "Asset copy completed for group '{}'. Copied: {}, Skipped: {}",
                    asset_id, copied_count, skipped_count
                ),
                1.0,
                None,
            )
            .await?;
        }

        info!(
            "[NRC Assets Copy '{}'] Completed. Copied: {}, Skipped: {}",
            asset_id, copied_count, skipped_count
        );
        Ok(())
    }

    /// Helper method for emitting copy progress events
    async fn emit_copy_event(
        &self,
        state: &State,
        profile_id: Uuid,
        message: &str,
        progress: f64,
        error: Option<String>,
    ) -> Result<Uuid> {
        let event_id = Uuid::new_v4();
        state
            .emit_event(EventPayload {
                event_id,
                event_type: EventType::CopyingNoRiskClientAssets,
                target_id: Some(profile_id),
                message: message.to_string(),
                progress: Some(progress.clamp(0.0, 1.0)),
                error,
            })
            .await?;
        Ok(event_id)
    }

    /// Recursively deletes files and directories within `base_dir` that are not present in `expected_paths`.
    /// Processes directories bottom-up to ensure empty directories can be removed.
    async fn cleanup_orphan_assets(
        &self,
        base_dir: &Path,
        expected_paths: &HashSet<PathBuf>,
    ) -> Result<usize> {
        if !base_dir.exists() {
            info!(
                "[NRC Assets Cleanup] Base directory {} does not exist. Nothing to clean.",
                base_dir.display()
            );
            return Ok(0);
        }

        let entries_to_check = vec![base_dir.to_path_buf()];
        let dirs_to_delete_later: Vec<PathBuf> = Vec::new();
        let mut deleted_count = 0;

        // Perform a breadth-first traversal to collect all paths
        let mut all_paths = HashSet::new();
        let mut queue = vec![base_dir.to_path_buf()];

        while let Some(current_path) = queue.pop() {
            if !current_path.exists() {
                continue;
            } // Skip if path was deleted during the process

            if all_paths.insert(current_path.clone()) {
                if current_path.is_dir() {
                    let mut reader = match fs::read_dir(&current_path).await {
                        Ok(r) => r,
                        Err(e) => {
                            warn!(
                                "[NRC Assets Cleanup] Failed to read directory {}: {}. Skipping.",
                                current_path.display(),
                                e
                            );
                            continue;
                        }
                    };
                    while let Some(entry_result) = reader.next_entry().await.transpose() {
                        match entry_result {
                            Ok(entry) => {
                                queue.push(entry.path());
                            }
                            Err(e) => {
                                warn!("[NRC Assets Cleanup] Failed to read entry in {}: {}. Skipping entry.", current_path.display(), e);
                            }
                        }
                    }
                }
            }
        }

        // Sort paths by depth (longest first) to process files/inner dirs before outer dirs
        let mut sorted_paths: Vec<PathBuf> = all_paths.into_iter().collect();
        sorted_paths.sort_by_key(|b| std::cmp::Reverse(b.components().count()));

        for path in sorted_paths {
            // Skip the base directory itself from deletion check if it's expected
            if path == base_dir && expected_paths.contains(base_dir) {
                continue;
            }

            if !expected_paths.contains(&path) {
                if path.is_file() {
                    debug!(
                        "[NRC Assets Cleanup] Deleting orphan file: {}",
                        path.display()
                    );
                    match fs::remove_file(&path).await {
                        Ok(_) => deleted_count += 1,
                        Err(e) => error!(
                            "[NRC Assets Cleanup] Failed to delete file {}: {}",
                            path.display(),
                            e
                        ),
                    }
                } else if path.is_dir() {
                    // Attempt to delete directory - might fail if not empty due to previous file deletion errors
                    debug!(
                        "[NRC Assets Cleanup] Deleting orphan directory: {}",
                        path.display()
                    );
                    match fs::remove_dir(&path).await {
                        Ok(_) => deleted_count += 1,
                        Err(e) => {
                            // Log error (likely dir not empty or permission issue)
                            // Don't stop the whole process, just log it.
                            warn!("[NRC Assets Cleanup] Failed to delete directory {} (may not be empty?): {}", path.display(), e);
                        }
                    }
                }
            }
        }

        Ok(deleted_count)
    }
}
