use crate::config::{ProjectDirsExt, HTTP_CLIENT, LAUNCHER_DIRECTORY};
use crate::error::Result;
use crate::minecraft::dto::quilt_meta::QuiltVersionInfo;
use log::{debug, error};
use serde_json;
use std::path::PathBuf;
use tokio::fs as tokio_fs;

pub struct QuiltApi {
    base_url: String,
    cache_dir: PathBuf,
}

impl QuiltApi {
    pub fn new() -> Self {
        let cache_dir = LAUNCHER_DIRECTORY.meta_dir().join("quilt_cache");
        if !cache_dir.exists() {
            std::fs::create_dir_all(&cache_dir).unwrap_or_else(|e| {
                error!("Failed to create Quilt cache directory: {}", e);
            });
        }
        Self {
            base_url: "https://meta.quiltmc.org/v3".to_string(),
            cache_dir,
        }
    }

    async fn fetch_and_cache_versions(
        base_url: &str,
        minecraft_version: &str,
        cache_path: &PathBuf,
    ) -> Result<Vec<QuiltVersionInfo>> {
        let url = format!("{}/versions/loader/{}", base_url, minecraft_version);
        debug!("Fetching Quilt versions from: {}", url);

        let response = HTTP_CLIENT.get(&url).send().await.map_err(|e| {
            crate::error::AppError::QuiltError(format!("Failed to fetch Quilt versions: {}", e))
        })?;

        if !response.status().is_success() {
            return Err(crate::error::AppError::QuiltError(format!(
                "Failed to fetch Quilt versions: Status {}",
                response.status()
            )));
        }

        let versions = response
            .json::<Vec<QuiltVersionInfo>>()
            .await
            .map_err(|e| {
                crate::error::AppError::QuiltError(format!("Failed to parse Quilt versions: {}", e))
            })?;

        let json_data = serde_json::to_string_pretty(&versions).map_err(|e| {
            crate::error::AppError::QuiltError(format!("Failed to serialize versions: {}", e))
        })?;

        if let Err(e) = tokio_fs::write(cache_path, json_data).await {
            error!("Failed to write Quilt cache: {}", e);
        } else {
            debug!("Cached Quilt versions for {}: {:?}", minecraft_version, cache_path);
        }

        Ok(versions)
    }

    async fn background_update(
        base_url: String,
        minecraft_version: String,
        cache_path: PathBuf,
    ) {
        debug!("[BG] Updating Quilt versions for {}", minecraft_version);
        if let Err(e) = Self::fetch_and_cache_versions(&base_url, &minecraft_version, &cache_path).await {
            error!("[BG] Failed to update Quilt cache for {}: {}", minecraft_version, e);
        }
    }

    pub async fn get_loader_versions(
        &self,
        minecraft_version: &str,
    ) -> Result<Vec<QuiltVersionInfo>> {
        let cache_filename = format!("quilt_versions_{}.json", minecraft_version);
        let cache_path = self.cache_dir.join(&cache_filename);

        if cache_path.exists() {
            debug!("Cache hit for Quilt versions {}: {:?}", minecraft_version, cache_path);
            
            match tokio_fs::read_to_string(&cache_path).await {
                Ok(cached_data) => {
                    match serde_json::from_str::<Vec<QuiltVersionInfo>>(&cached_data) {
                        Ok(cached_versions) => {
                            let base_url = self.base_url.clone();
                            let minecraft_version = minecraft_version.to_string();
                            let cache_path_clone = cache_path.clone();
                            
                            tokio::spawn(async move {
                                Self::background_update(base_url, minecraft_version, cache_path_clone).await;
                            });
                            
                            return Ok(cached_versions);
                        }
                        Err(e) => {
                            error!("Failed to parse cached Quilt data: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to read Quilt cache: {}", e);
                }
            }
        }

        debug!("Cache miss for Quilt versions {}, fetching...", minecraft_version);
        Self::fetch_and_cache_versions(&self.base_url, minecraft_version, &cache_path).await
    }

    pub async fn get_latest_stable_version(
        &self,
        minecraft_version: &str,
    ) -> Result<QuiltVersionInfo> {
        let versions = self.get_loader_versions(minecraft_version).await?;

        versions
            .into_iter()
            .filter(|v| v.loader.stable)
            .max_by_key(|v| v.loader.build)
            .ok_or_else(|| {
                crate::error::AppError::QuiltError("No stable Quilt version found".to_string())
            })
    }
}
