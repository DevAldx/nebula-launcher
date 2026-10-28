import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api } from "./api";
import { accentColors } from "./color";
import type {
  Account, AppInfo, AuthStateEvent, GameState, GameStateEvent, ImportStateEvent, Instance,
  InstanceList, JavaRuntime, LogEvent, ProgressEvent, Settings,
} from "./types";

const MAX_LOG_LINES = 1500;

/** Estado global mínimo. La fuente de verdad siempre es Rust. */
class AppState {
  settings = $state<Settings | null>(null);
  info = $state<AppInfo | null>(null);
  list = $state<InstanceList>({ instances: [], broken: [] });
  java = $state<JavaRuntime[]>([]);
  javaLoading = $state(false);
  ready = $state(false);
  fatal = $state<string | null>(null);

  /** Estado de juego por ID de instancia (ausente = inactiva). */
  games = $state<Record<string, GameState>>({});
  progress = $state<Record<string, ProgressEvent>>({});
  errors = $state<Record<string, string>>({});
  logs = $state<Record<string, LogEvent[]>>({});

  // Importar por código.
  importBusy = $state(false);
  importError = $state<string | null>(null);
  importedId = $state<string | null>(null);

  // Cuenta local sin Microsoft/Azure.
  localSignInOpen = $state(false);
  localSignInBusy = $state(false);
  localSignInError = $state<string | null>(null);

  // Cuenta de Microsoft.
  account = $state<Account | null>(null);
  signIn = $state<{ userCode: string; verificationUri: string } | null>(null);
  signInBusy = $state(false);
  signInError = $state<string | null>(null);
  /** `true` mientras el diálogo usa el flujo heredado (no oficial). */
  signInLegacy = $state(false);

  selected = $derived<Instance | null>(
    this.list.instances.find((i) => i.id === this.settings?.selected_instance) ?? null,
  );

  /** `data:` URL por instancia; ausente = sin cargar, `null` = sin fondo propio. */
  backgrounds = $state<Record<string, string | null>>({});
  private backgroundsLoading = new Set<string>();

  /** Fondo e icono propios del launcher (no de una instancia). */
  launcherBackground = $state<string | null>(null);
  launcherIcon = $state<string | null>(null);

  async ensureBackground(instanceId: string) {
    if (instanceId in this.backgrounds || this.backgroundsLoading.has(instanceId)) return;
    this.backgroundsLoading.add(instanceId);
    try {
      const bg = await api.getInstanceBackground(instanceId);
      this.backgrounds[instanceId] = bg ? `data:${bg.mime};base64,${bg.data_base64}` : null;
    } catch {
      this.backgrounds[instanceId] = null;
    } finally {
      this.backgroundsLoading.delete(instanceId);
    }
  }

  invalidateBackground(instanceId: string) {
    delete this.backgrounds[instanceId];
  }

  async loadLauncherAppearance() {
    try {
      const [background, icon] = await Promise.all([
        api.getLauncherBackground(),
        api.getLauncherIcon(),
      ]);
      this.launcherBackground = background
        ? `data:${background.mime};base64,${background.data_base64}`
        : null;
      this.launcherIcon = icon ? `data:${icon.mime};base64,${icon.data_base64}` : null;
    } catch (e) {
      console.error("no se pudo cargar la apariencia del launcher", e);
    }
  }

  async setLauncherBackground(mime: string, dataBase64: string) {
    await api.setLauncherBackground(mime, dataBase64);
    this.launcherBackground = `data:${mime};base64,${dataBase64}`;
  }

  async clearLauncherBackground() {
    await api.clearLauncherBackground();
    this.launcherBackground = null;
  }

  async setLauncherIcon(dataBase64: string) {
    await api.setLauncherIcon("image/png", dataBase64);
    this.launcherIcon = `data:image/png;base64,${dataBase64}`;
  }

  async clearLauncherIcon() {
    await api.clearLauncherIcon();
    this.launcherIcon = null;
  }

  private unlisten: UnlistenFn[] = [];

  async init() {
    try {
      this.unlisten = await Promise.all([
        listen<GameStateEvent>("game-state", (e) => this.onGameState(e.payload)),
        listen<ProgressEvent>("install-progress", (e) => (this.progress[e.payload.id] = e.payload)),
        listen<LogEvent>("game-log", (e) => this.onLog(e.payload)),
        listen<AuthStateEvent>("auth-state", (e) => this.onAuthState(e.payload)),
        listen<ImportStateEvent>("import-state", (e) => this.onImportState(e.payload)),
      ]);
      [this.settings, this.info, this.list, this.account] = await Promise.all([
        api.getSettings(), api.appInfo(), api.listInstances(), api.getAccount(),
      ]);
      // Al copiar una instancia a otro PC, `settings.json` no viaja con ella y
      // no hay ninguna seleccionada. Elegir la más reciente evita que Inicio
      // parezca vacío o que haya que «configurar» algo antes de jugar.
      const selected = this.list.instances.some(
        (i) => i.id === this.settings?.selected_instance,
      );
      if (!selected && this.list.instances.length > 0) {
        const newest = [...this.list.instances].sort(
          (a, b) => (b.last_played ?? 0) - (a.last_played ?? 0),
        )[0];
        if (newest) await this.select(newest.id);
      }
      const running = await api.gameStates();
      for (const [id, s] of Object.entries(running)) this.games[id] = s as GameState;
      this.ready = true;
      void this.loadLauncherAppearance();
      void this.detectJava();
    } catch (e) {
      this.fatal = e instanceof Error ? e.message : String(e);
    }
  }

  private onGameState(ev: GameStateEvent) {
    if (ev.state === "preparing") {
      delete this.errors[ev.id];
      this.logs[ev.id] = [];
    }
    if (ev.state === "stopped" || ev.state === "error") {
      delete this.games[ev.id];
      delete this.progress[ev.id];
      if (ev.state === "error" && ev.message) {
        this.errors[ev.id] = ev.message;
        // Si la sesión de Microsoft caducó, Rust borra el perfil cacheado;
        // refrescarlo evita seguir mostrando «conectado».
        void this.refreshAccount();
      }
      if (ev.state === "stopped" && ev.exit_code) {
        this.errors[ev.id] = `El juego terminó con código ${ev.exit_code}. Revisa los logs.`;
      }
      void this.refreshInstances(); // actualiza «instalada»
    } else {
      this.games[ev.id] = ev.state;
    }
  }

  private onLog(ev: LogEvent) {
    const lines = this.logs[ev.id] ?? (this.logs[ev.id] = []);
    lines.push(ev);
    if (lines.length > MAX_LOG_LINES) lines.splice(0, lines.length - MAX_LOG_LINES);
  }

  private onAuthState(ev: AuthStateEvent) {
    if (ev.state === "connecting") {
      this.signInBusy = true;
      return;
    }
    this.signInBusy = false;
    if (ev.state === "success") {
      this.account = ev.account;
      this.signIn = null;
      this.signInError = null;
      this.signInLegacy = false;
    } else if (ev.state === "cancelled") {
      this.signIn = null;
      this.signInError = null;
      this.signInLegacy = false;
    } else if (ev.state === "error") {
      this.signIn = null;
      this.signInError = ev.message;
      this.signInLegacy = false;
    }
  }

  private onImportState(ev: ImportStateEvent) {
    this.importBusy = false;
    if (ev.state === "success") {
      this.importError = null;
      this.importedId = ev.instance_id;
      void this.refreshInstances();
      void this.detectJava(); // puede haberse instalado un JRE nuevo
    } else {
      this.importError = ev.message;
    }
  }

  async importFromCode(code: string) {
    this.importBusy = true;
    this.importError = null;
    this.importedId = null;
    try {
      await api.importFromCode(code);
      // el resultado llega por el evento `import-state`
    } catch (e) {
      this.importBusy = false;
      this.importError = e instanceof Error ? e.message : String(e);
    }
  }

  async detectJava() {
    this.javaLoading = true;
    try {
      this.java = await api.listJava();
    } finally {
      this.javaLoading = false;
    }
  }

  async refreshInstances() {
    // Un refresco en segundo plano no debe romper la UI ni generar
    // rechazos de promesa sin capturar si Rust está ocupado.
    try {
      this.list = await api.listInstances();
    } catch (e) {
      console.error("no se pudieron refrescar las instancias", e);
    }
  }

  /** Relee la cuenta guardada; si Rust la descartó, deja de mostrarla. */
  async refreshAccount() {
    try {
      this.account = await api.getAccount();
    } catch (e) {
      console.error("no se pudo refrescar la cuenta", e);
    }
  }

  async saveSettings(next: Settings) {
    this.settings = await api.saveSettings(next);
  }

  async select(id: string | null) {
    try {
      this.settings = await api.selectInstance(id);
    } catch (e) {
      console.error("no se pudo seleccionar la instancia", e);
    }
  }

  async play(id: string) {
    try {
      delete this.errors[id];
      await api.play(id);
    } catch (e) {
      this.errors[id] = e instanceof Error ? e.message : String(e);
      void this.refreshAccount();
    }
  }

  async stop(id: string) {
    try {
      await api.stop(id);
    } catch (e) {
      this.errors[id] = e instanceof Error ? e.message : String(e);
    }
  }

  async startSignIn() {
    this.signInError = null;
    this.signInLegacy = false;
    this.signInBusy = true;
    try {
      const code = await api.beginSignIn();
      // Si Rust abrió la ventana dedicada, no mostramos el modal sobre la UI.
      this.signIn = code.window_opened
        ? null
        : { userCode: code.user_code, verificationUri: code.verification_uri };
    } catch (e) {
      this.signInError = e instanceof Error ? e.message : String(e);
      this.signInBusy = false;
    }
  }

  /** Flujo heredado de login.live.com, sin app de Azure (no oficial). */
  async startLegacySignIn() {
    this.signInError = null;
    this.signInLegacy = true;
    this.signInBusy = true;
    try {
      const code = await api.beginLegacySignIn();
      this.signIn = code.window_opened
        ? null
        : { userCode: code.user_code, verificationUri: code.verification_uri };
    } catch (e) {
      this.signInError = e instanceof Error ? e.message : String(e);
      this.signInBusy = false;
      this.signInLegacy = false;
    }
  }

  openLocalSignIn() {
    // Una cuenta local y un inicio de sesión de Microsoft no deben competir:
    // si hay uno de Microsoft en curso, se cancela al abrir este diálogo.
    if (this.signInBusy) void this.cancelSignIn();
    this.localSignInError = null;
    this.localSignInOpen = true;
  }

  closeLocalSignIn() {
    if (this.localSignInBusy) return;
    this.localSignInOpen = false;
    this.localSignInError = null;
  }

  async signInOffline(username: string) {
    this.localSignInBusy = true;
    this.localSignInError = null;
    try {
      this.account = await api.signInOffline(username);
      this.localSignInOpen = false;
    } catch (e) {
      this.localSignInError = e instanceof Error ? e.message : String(e);
    } finally {
      this.localSignInBusy = false;
    }
  }

  async cancelSignIn() {
    this.signIn = null;
    this.signInBusy = false;
    this.signInLegacy = false;
    this.signInError = null;
    try {
      await api.cancelSignIn();
    } catch {
      // da igual si ya había terminado por su cuenta
    }
  }

  async signOut() {
    await api.signOut();
    this.account = null;
  }
}

export const app = new AppState();

/** Aplica tema, color de acento y preferencias visuales al documento. */
export function applyUi(ui: Settings["ui"]) {
  const root = document.documentElement;
  root.dataset.theme = ui.theme;
  root.dataset.animations = String(ui.animations);
  root.dataset.transparency = String(ui.transparency);

  const accent = accentColors(ui.accent);
  if (accent) {
    root.style.setProperty("--accent", accent.accent);
    root.style.setProperty("--accent-hover", accent.hover);
    root.style.setProperty("--accent-glow", accent.glow);
  } else {
    // Deja que el tema vuelva a mandar (los valores viven en app.css).
    root.style.removeProperty("--accent");
    root.style.removeProperty("--accent-hover");
    root.style.removeProperty("--accent-glow");
  }
}
