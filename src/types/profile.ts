import { ContentType } from "./content";

export type ModLoader = "vanilla" | "forge" | "fabric" | "quilt" | "neoforge";
export type ProfileState =
  | "not_installed"
  | "installing"
  | "installed"
  | "running"
  | "error";

interface ImageSourceBase {
  type: "url" | "relativePath" | "relativeProfile" | "absolutePath" | "base64";
}

export interface ImageSourceUrl extends ImageSourceBase {
  type: "url";
  url: string;
}

export interface ImageSourceRelativePath extends ImageSourceBase {
  type: "relativePath";
  path: string;
}

export interface ImageSourceRelativeProfile extends ImageSourceBase {
  type: "relativeProfile";
  path: string;
}

export interface ImageSourceAbsolutePath extends ImageSourceBase {
  type: "absolutePath";
  path: string;
}

export interface ImageSourceBase64 extends ImageSourceBase {
  type: "base64";
  data: string;
  mime_type?: string;
}

export type ImageSource =
  | ImageSourceUrl
  | ImageSourceRelativePath
  | ImageSourceRelativeProfile
  | ImageSourceAbsolutePath
  | ImageSourceBase64;

export interface ProfileBanner {
  source: ImageSource;
}

export interface MemorySettings {
  min: number;
  max: number;
}

export interface WindowSize {
  width: number;
  height: number;
}

export interface ProfileSettings {
  java_path: string | null;       // Option<String> -> string | null
  use_custom_java_path: boolean; // Added boolean flag
  memory: MemorySettings;
  resolution: WindowSize | null;
  fullscreen: boolean;
  extra_game_args: string[];           // Vec<String> -> string[] (Renamed from extra_args)
  custom_jvm_args: string | null;   // Option<String> -> string | null (New)
}

interface ModSourceBase {
  type: "local" | "url" | "maven" | "embedded" | "modrinth";
}

export interface ModSourceLocal extends ModSourceBase {
  type: "local";
  file_name: string;
}

export interface ModSourceUrl extends ModSourceBase {
  type: "url";
  url: string;
  file_name: string | null;
}

export interface ModSourceMaven extends ModSourceBase {
  type: "maven";
  coordinates: string;
  repository_url: string | null;
}

export interface ModSourceEmbedded extends ModSourceBase {
  type: "embedded";
  name: string;
}

export interface ModSourceModrinth extends ModSourceBase {
  type: "modrinth";
  project_id: string;
  version_id: string;
  file_name: string;
  download_url: string;
  file_hash_sha1: string | null;
}

export type ModSource =
  | ModSourceLocal
  | ModSourceUrl
  | ModSourceMaven
  | ModSourceEmbedded
  | ModSourceModrinth;

export interface Mod {
  id: string;
  source: ModSource;
  enabled: boolean;
  display_name: string | null;
  version: string | null;
  game_versions: string[] | null;
  file_name_override: string | null;
  associated_loader: ModLoader | null;
}

export interface NoriskModIdentifier {
  pack_id: string;
  mod_id: string;
  game_version: string;
  loader: ModLoader;
}

export interface NoriskInformation {
  keep_local_assets: boolean;
  is_experimental: boolean;
}

export interface CustomModInfo {
  filename: string;
  is_enabled: boolean;
  path: string;
}

export interface Profile {
  id: string;
  name: string;
  path: string;
  game_version: string;
  loader: ModLoader;
  loader_version: string | null;
  created: string;
  last_played: string | null;
  settings: ProfileSettings;
  state: ProfileState;
  mods: Mod[];
  selected_norisk_pack_id: string | null;
  disabled_norisk_mods_detailed: NoriskModIdentifier[];
  source_standard_profile_id: string | null;
  group: string | null;
  is_standard_version: boolean;
  description: string | null;
  banner: ProfileBanner | null;
  background: ProfileBanner | null;
  norisk_information: NoriskInformation | null;
}

export interface ProfileGroup {
  id: string;
  name: string;
  profiles: string[];
}

export type ProfileFilterType = "all" | "custom" | "standard";

export interface CreateProfileParams {
  name: string;
  game_version: string;
  loader: string;
  loader_version?: string;
  selected_norisk_pack_id?: string;
}

export interface UpdateProfileParams {
  name?: string;
  game_version?: string;
  loader?: string;
  loader_version?: string;
  settings?: ProfileSettings;
  selected_norisk_pack_id?: string;
  group?: string | null;
  description?: string | null;
  clear_selected_norisk_pack?: boolean;
  banner?: ProfileBanner | null;
  background?: ProfileBanner | null;
  norisk_information?: NoriskInformation | null;
}

export interface CopyProfileParams {
  source_profile_id: string;
  new_profile_name: string;
  include_files?: string[];
}

export interface ExportProfileParams {
  profile_id: string;
  file_name: string;
  include_files?: string[];
  open_folder: boolean;
}

// --- Payload for upload_profile_icon command ---
export interface UploadProfileIconPayload {
  path?: string;      // Source path of the image file (optional)
  profileId: string; // UUID of the profile (as string)
  imageType: string; // "icon" or "background"
}

// --- Types for Commands ---

/**
 * Parameters for the `copy_world` Tauri command.
 */
export interface CopyWorldParams {
  source_profile_id: string; // Uuid
  source_world_folder: string;
  target_profile_id: string; // Uuid
  target_world_name: string;
}

// --- Types for check_content_installed command ---

/**
 * Parameters for the `is_content_installed` Tauri command.
 */
export interface CheckContentParams {
  profile_id: string; // Uuid -> string
  project_id?: string | null;
  version_id?: string | null;
  file_hash_sha1?: string | null;
  file_name?: string | null;
  project_type?: string | null;
  game_version?: string | null;
  loader?: string | null;
  pack_version_number?: string | null;
}

/**
 * Return type for the `is_content_installed` Tauri command.
 */
export interface FoundItemDetails {
  item_type: ContentType;
  item_id?: string;
  file_name?: string;
  display_name?: string;
}

/**
 * Details about an item when it comes from a NoRisk Pack
 */
export interface NoRiskPackItemDetails {
  is_enabled: boolean;
  norisk_mod_identifier?: NoriskModIdentifier;
}

export interface ContentInstallStatus {
  is_included_in_norisk_pack: boolean;
  is_installed: boolean;
  is_specific_version_in_pack: boolean;
  is_enabled?: boolean;
  found_item_details?: FoundItemDetails;
  norisk_pack_item_details?: NoRiskPackItemDetails;
}

/**
 * Request parameters for a single content item in a batch check
 */
export interface ContentCheckRequest {
  project_id?: string | null;
  version_id?: string | null;
  file_hash_sha1?: string | null;
  file_name?: string | null;
  project_type?: string | null;
  game_version?: string | null;
  loader?: string | null;
  pack_version_number?: string | null;
  request_id?: string | null; // Optional client ID to match requests with responses
}

/**
 * Parameters for the `batch_check_content_installed` Tauri command.
 */
export interface BatchCheckContentParams {
  profile_id: string; // Uuid -> string
  requests: ContentCheckRequest[];
}

/**
 * Result for a single content check request in the batch response
 */
export interface ContentCheckResult {
  request_id?: string | null; // Same ID that was provided in the request
  status: ContentInstallStatus;
  project_id?: string | null;
  version_id?: string | null;
  file_name?: string | null;
  project_type?: string | null;
}

/**
 * Return type for the `batch_check_content_installed` Tauri command.
 */
export interface BatchContentInstallStatus {
  results: ContentCheckResult[];
}

// Added: Type for Screenshot Information
export interface ScreenshotInfo {
  filename: string;
  path: string;
  modified: string | null; // DateTime<Utc> -> string (ISO 8601) | null
}

// --- New Type for All Profiles and Last Played ---
export interface AllProfilesAndLastPlayed {
  all_profiles: Profile[];
  last_played_profile_id: string | null;
}

// --- Generic Content Types ---

// Ensure ContentType enum/type is comprehensive if not already defined elsewhere
// For this example, assuming it's similar to the Rust enum and defined in ./content.ts
// export enum ContentType { ResourcePack, ShaderPack, DataPack, Mod }

export interface GenericModrinthInfo {
  project_id: string;
  version_id: string;
  name: string;
  version_number: string;
  download_url?: string | null; // Making it optional as in Rust struct
}

export interface LocalContentItem {
  filename: string;
  path_str: string; 
  sha1_hash?: string | null;
  file_size: number; // u64 in Rust maps to number in TS
  is_disabled: boolean;
  is_directory: boolean;
  content_type: ContentType; 
  modrinth_info?: GenericModrinthInfo | null;
  source_type?: string | null; // For identifying "custom" mods
  norisk_info?: NoriskModIdentifier | null; // Identifier for NoRiskMods
  fallback_version?: string | null; // Fallback version from compatibility target
  id?: string | null; // Added optional ID field from ModProfileEntry.id
  associated_loader?: ModLoader | null; // Added associated_loader from ModProfileEntry
  // Frontend specific fields can be added here if needed, e.g., for UI state
  // local_icon_data_url?: string; // Example if we were to add this later
}

// --- Params for get_local_content command ---
export interface LoadItemsParams {
  profile_id: string; // UUID
  content_type: ContentType; // Enum: ResourcePack, ShaderPack, DataPack
  calculate_hashes: boolean;
  fetch_modrinth_data: boolean;
}
