import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  Instance,
  JavaRuntime,
  InstanceList,
  ModEntry,
  UpdateCheck,
  BackgroundView,
  InstanceManifest,
  NewInstance,
  Settings,
  UpdateInstance,
  VersionSummary,
  Account,
  SignInCodeEvent,
} from "./types";

/** Los comandos de Rust fallan con `{ message }`; lo normalizamos a Error. */
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    const msg =
      typeof e === "object" && e !== null && "message" in e
        ? String((e as { message: unknown }).message)
        : String(e);
    throw new Error(msg);
  }
}

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  getSettings: () => call<Settings>("get_settings"),
  saveSettings: (settings: Settings) => call<Settings>("save_settings", { settings }),
  listInstances: () => call<InstanceList>("list_instances"),
  createInstance: (input: NewInstance) => call<Instance>("create_instance", { input }),
  updateInstance: (id: string, patch: UpdateInstance) =>
    call<Instance>("update_instance", { id, patch }),
  deleteInstance: (id: string) => call<Settings>("delete_instance", { id }),
  getAccount: () => call<Account | null>("get_account"),
  getSignInCode: () => call<SignInCodeEvent | null>("get_sign_in_code"),
  beginSignIn: () => call<SignInCodeEvent>("begin_sign_in"),
  beginLegacySignIn: () => call<SignInCodeEvent>("begin_legacy_sign_in"),
  signInOffline: (username: string) => call<Account>("sign_in_offline", { username }),
  cancelSignIn: () => call<void>("cancel_sign_in"),
  signOut: () => call<void>("sign_out"),
  listJava: () => call<JavaRuntime[]>("list_java_runtimes"),
  listVersions: () => call<VersionSummary[]>("list_minecraft_versions"),
  play: (id: string) => call<void>("play_instance", { id }),
  stop: (id: string) => call<void>("stop_instance", { id }),
  gameStates: () => call<Record<string, string>>("game_states"),
  openUrl: (url: string) => call<void>("open_url", { url }),
  openHttpsUrl: (url: string) => call<void>("open_https_url", { url }),
  checkForUpdates: () => call<UpdateCheck>("check_for_updates"),
  getInstanceBackground: (instanceId: string) => call<BackgroundView | null>("get_instance_background", { instanceId }),
  setInstanceBackground: (instanceId: string, mime: string, dataBase64: string) =>
    call<void>("set_instance_background", { instanceId, mime, dataBase64 }),
  clearInstanceBackground: (instanceId: string) => call<void>("clear_instance_background", { instanceId }),
  fetchShareManifest: (code: string) => call<InstanceManifest>("fetch_share_manifest", { code }),
  importFromCode: (code: string) => call<void>("import_from_code", { code }),
  exportManifestTemplate: (instanceId: string) => call<string>("export_manifest_template", { instanceId }),
  listMods: (instanceId: string) => call<ModEntry[]>("list_mods", { instanceId }),
  setModEnabled: (instanceId: string, fileName: string, enabled: boolean) =>
    call<string>("set_mod_enabled", { instanceId, fileName, enabled }),
  deleteMod: (instanceId: string, fileName: string) =>
    call<void>("delete_mod", { instanceId, fileName }),
  importMods: (instanceId: string, files: { name: string; data_base64: string }[]) =>
    call<string[]>("import_mods", { instanceId, files }),
  openInstanceFolder: (instanceId: string, sub: "" | "mods" | "config") =>
    call<void>("open_instance_folder", { instanceId, sub }),
  selectInstance: (id: string | null) => call<Settings>("select_instance", { id }),
  checkAndPrepareUpdate: () => call<NebulaUpdateStatus>("check_and_prepare_update"),
  installUpdate: () => call<void>("install_update"),
};

/** "-Xmx2G -XX:+UseG1GC" → ["-Xmx2G", "-XX:+UseG1GC"] */
export const splitArgs = (s: string): string[] => s.split(/\s+/).filter(Boolean);
export const joinArgs = (a: string[]): string => a.join(" ");

export type NebulaUpdateInfo = {
  version: string;
  current_version: string;
  notes?: string | null;
};

export type NebulaUpdateStatus = {
  available: boolean;
  update: NebulaUpdateInfo | null;
};
