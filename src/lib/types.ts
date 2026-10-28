// Espejo de los tipos serializados por nebula-core.

export type Theme = "dark" | "black" | "purple";
export type BackgroundFit = "cover" | "contain" | "tile";
export type Loader = "vanilla" | "fabric";

export interface Memory {
  min_mb: number;
  max_mb: number;
}

export interface Settings {
  schema_version: number;
  data_dir: string | null;
  selected_instance: string | null;
  minecraft: {
    min_ram_mb: number;
    max_ram_mb: number;
    jvm_args: string[];
    width: number;
    height: number;
    fullscreen: boolean;
  };
  ui: {
    theme: Theme;
    animations: boolean;
    transparency: boolean;
    accent: string | null;
    background_fit: BackgroundFit;
    background_blur: number;
    background_dim: number;
  };
  account: { microsoft_client_id: string | null };
  updates: { manifest_url: string | null };
  sharing: { manifest_base_url: string | null };
}

export type JavaChoice = { kind: "auto" } | { kind: "path"; path: string };

export interface Instance {
  schema_version: number;
  id: string;
  name: string;
  minecraft_version: string;
  loader: Loader;
  loader_version: string | null;
  java: JavaChoice;
  memory: Memory;
  jvm_args: string[];
  created_at: number;
  last_played: number | null;
  total_playtime_secs: number;
  mod_count: number;
  installed: boolean;
}

export interface BackgroundView {
  mime: string;
  data_base64: string;
}

export interface InstanceList {
  instances: Instance[];
  broken: { id: string; error: string }[];
}

export interface NewInstance {
  name: string;
  minecraft_version: string;
  loader: Loader;
  memory: Memory;
  jvm_args: string[];
}

export interface ModEntry {
  file_name: string;
  enabled: boolean;
  size_bytes: number;
}

export interface UpdateInstance {
  name?: string;
  memory?: Memory;
  jvm_args?: string[];
  java?: JavaChoice;
}

export interface AppInfo {
  version: string;
  data_dir: string;
  config_dir: string;
}

export interface JavaRuntime {
  path: string;
  major: number;
  version: string;
}

export interface VersionSummary {
  id: string;
  kind: string;
  release_time: string | null;
}

export type GameState = "preparing" | "running" | "stopped" | "error";

export interface GameStateEvent {
  id: string;
  state: GameState;
  message: string | null;
  exit_code: number | null;
}

export interface ProgressEvent {
  id: string;
  phase: string;
  done: number;
  total: number;
}

export interface LogEvent {
  id: string;
  stream: "stdout" | "stderr";
  text: string;
}

export type AccountKind = "microsoft" | "offline";
export type AuthFlow = "azure" | "legacy";

export interface Account {
  uuid: string;
  username: string;
  skin_url: string | null;
  kind: AccountKind;
  flow: AuthFlow;
}

export interface SignInCodeEvent {
  user_code: string;
  verification_uri: string;
  legacy: boolean;
  window_opened: boolean;
}

export type AuthState = "connecting" | "success" | "cancelled" | "error";

export interface AuthStateEvent {
  state: AuthState;
  message: string | null;
  account: Account | null;
}

export interface UpdateManifest {
  version: string;
  url: string;
  notes: string | null;
}

export interface UpdateCheck {
  current_version: string;
  available: UpdateManifest | null;
}

export interface ManifestFile {
  file_name: string;
  url: string;
  sha1: string | null;
  size: number | null;
}

export interface InstanceManifest {
  code: string;
  name: string;
  description: string | null;
  icon_url: string | null;
  background_url: string | null;
  minecraft_version: string;
  loader: Loader;
  loader_version: string | null;
  java: { min_major: number } | null;
  memory: Memory | null;
  mods: ManifestFile[];
  resourcepacks: ManifestFile[];
  shaderpacks: ManifestFile[];
  config_files: ManifestFile[];
}

export type ImportState = "success" | "error";

export interface ImportStateEvent {
  state: ImportState;
  instance_id: string | null;
  message: string | null;
}
