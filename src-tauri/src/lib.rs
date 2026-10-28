//! Capa fina de Tauri: cada comando valida su entrada y delega en `nebula-core`.
//! El frontend nunca toca el disco ni ejecuta nada por su cuenta.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};

use nebula_core::{
    auth::{self, Account, AccountKind, AccountStore, AuthEndpoints, AuthFlow},
    config::Settings,
    instances::{
        InstanceId, InstanceList, InstanceStore, InstanceView, JavaChoice, Loader, NewInstance,
        UpdateInstance,
    },
    java::{self, JavaRuntime},
    launcher::{self, LaunchAuth, LaunchInput, Screen},
    background,
    minecraft::{fabric, install::copy_assets, Installer, Reporter, VersionSummary},
    mods::{self, ModEntry},
    share::{self, InstanceManifest},
    updater::{self, UpdateCheck},
    CoreError, NebulaPaths,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::UpdaterExt;
use tokio::sync::oneshot;

enum GameSlot {
    Preparing,
    Running(oneshot::Sender<()>),
}

struct AppState {
    paths: NebulaPaths,
    instances: InstanceStore,
    installer: Installer,
    settings: Mutex<Settings>,
    games: Mutex<HashMap<String, GameSlot>>,
    auth_store: AccountStore,
    auth_http: reqwest::Client,
    sign_in_cancel: Mutex<Option<oneshot::Sender<()>>>,
    sign_in_code: Mutex<Option<SignInCodeState>>,
}

impl AppState {
    fn settings(&self) -> Result<MutexGuard<'_, Settings>, CommandError> {
        self.settings
            .lock()
            .map_err(|_| CommandError::new("estado interno corrupto, reinicia Nebula"))
    }

    fn games(&self) -> Result<MutexGuard<'_, HashMap<String, GameSlot>>, CommandError> {
        self.games
            .lock()
            .map_err(|_| CommandError::new("estado interno corrupto, reinicia Nebula"))
    }

    fn sign_in_cancel(
        &self,
    ) -> Result<MutexGuard<'_, Option<oneshot::Sender<()>>>, CommandError> {
        self.sign_in_cancel
            .lock()
            .map_err(|_| CommandError::new("estado interno corrupto, reinicia Nebula"))
    }

    fn pending_sign_in(
        &self,
    ) -> Result<MutexGuard<'_, Option<SignInCodeState>>, CommandError> {
        self.sign_in_code
            .lock()
            .map_err(|_| CommandError::new("estado interno corrupto, reinicia Nebula"))
    }

    fn microsoft_client_id(&self) -> Result<String, CommandError> {
        let id = self
            .settings()?
            .account
            .microsoft_client_id
            .clone()
            .unwrap_or_default();
        if id.trim().is_empty() {
            return Err(CommandError::new(
                "configura un Client ID de Microsoft en Configuración > Cuenta antes de iniciar sesión (ver el README).",
            ));
        }
        Ok(id)
    }
}

/// Error serializable hacia el frontend (solo un mensaje legible).
#[derive(Debug, Serialize)]
struct CommandError {
    message: String,
}

impl CommandError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<CoreError> for CommandError {
    fn from(e: CoreError) -> Self {
        Self::new(e.to_string())
    }
}

// Límites de IPC: se comprueban ANTES de decodificar base64 para que una
// llamada enorme no duplique memoria (cadena + bytes) ni agote el proceso.
const MAX_BACKGROUND_B64: usize = 21 * 1024 * 1024; // 15 MB de imagen + margen
const MAX_ICON_B64: usize = 6 * 1024 * 1024; // 4 MB de PNG + margen
const MAX_MOD_B64: usize = 268 * 1024 * 1024; // 200 MB por mod + margen
const MAX_MOD_FILES: usize = 256;

type CmdResult<T> = Result<T, CommandError>;

// ───────────────────────────── Ajustes e info ─────────────────────────────

#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
    data_dir: String,
    config_dir: String,
}

#[tauri::command]
fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        data_dir: state.paths.data_dir().display().to_string(),
        config_dir: state.paths.config_dir().display().to_string(),
    }
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(state.settings()?.clone())
}

/// Guarda ajustes. La instancia seleccionada solo cambia con `select_instance`.
#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    let mut current = state.settings()?;
    let mut next = settings;
    next.schema_version = current.schema_version;
    next.selected_instance = current.selected_instance.clone();
    next.save(&state.paths.settings_file())?;
    *current = next.clone();
    Ok(next)
}

/// Sin `updates.manifest_url` configurada no comprueba nada (devuelve
/// `available: null`): no existe un feed propio todavía, ver el README.
#[tauri::command]
async fn check_for_updates(state: State<'_, AppState>) -> CmdResult<UpdateCheck> {
    let manifest_url = state
        .settings()?
        .updates
        .manifest_url
        .clone()
        .unwrap_or_default();
    if manifest_url.trim().is_empty() {
        return Ok(UpdateCheck {
            current_version: updater::CURRENT_VERSION.to_owned(),
            available: None,
        });
    }
    Ok(updater::check_for_update(&state.auth_http, &manifest_url).await?)
}

// ───────────────────────────── Java y versiones ─────────────────────────────

#[tauri::command]
async fn list_java_runtimes(state: State<'_, AppState>) -> CmdResult<Vec<JavaRuntime>> {
    Ok(java::detect(&state.paths).await)
}

#[tauri::command]
async fn list_minecraft_versions(state: State<'_, AppState>) -> CmdResult<Vec<VersionSummary>> {
    Ok(state.installer.version_list().await?)
}

// ───────────────────────────── Cuenta de Microsoft ─────────────────────────────

#[derive(Clone, Serialize)]
struct AccountView {
    uuid: String,
    username: String,
    skin_url: Option<String>,
    kind: AccountKind,
    flow: AuthFlow,
}

impl From<Account> for AccountView {
    fn from(a: Account) -> Self {
        Self {
            uuid: a.uuid,
            username: a.username,
            // Cuentas guardadas por 0.1.7 y anteriores pueden tener la URL http;
            // se normaliza al mostrarla para que la CSP no bloquee la skin.
            skin_url: a.skin_url.as_deref().map(auth::normalize_skin_url),
            kind: a.kind,
            flow: a.flow,
        }
    }
}

#[derive(Clone, Serialize)]
struct SignInCodeEvent {
    user_code: String,
    verification_uri: String,
    /// `true` si el código usa el flujo heredado de login.live.com.
    legacy: bool,
    /// `true` si el código se muestra en una ventana aparte (no en un modal).
    window_opened: bool,
}

/// Código pendiente que consulta la ventana de inicio de sesión al cargar.
#[derive(Clone)]
struct SignInCodeState {
    user_code: String,
    verification_uri: String,
    legacy: bool,
}

#[derive(Clone, Serialize)]
struct AuthStateEvent {
    /// `connecting`, `success`, `error`.
    state: &'static str,
    message: Option<String>,
    account: Option<AccountView>,
}

fn emit_auth_state(
    app: &AppHandle,
    state: &'static str,
    message: Option<String>,
    account: Option<AccountView>,
) {
    let _ = app.emit(
        "auth-state",
        AuthStateEvent {
            state,
            message,
            account,
        },
    );
}

/// Abre (o reutiliza) la ventana dedicada del inicio de sesión. No es un modal:
/// así el código no se mezcla con la interfaz principal.
fn open_sign_in_window(app: &AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    if let Some(window) = app.get_webview_window("signin") {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(window);
    }

    let window = tauri::WebviewWindowBuilder::new(
        app,
        "signin",
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("Nebula — Iniciar sesión")
    .inner_size(460.0, 500.0)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .center()
    .build()?;

    if let Some(icon) = app.default_window_icon().cloned() {
        let _ = window.set_icon(icon);
    }
    Ok(window)
}

fn close_sign_in_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("signin") {
        let _ = window.close();
    }
}

fn emit_sign_in_code(app: &AppHandle, event: &SignInCodeEvent) {
    let _ = app.emit_to("signin", "sign-in-code", event.clone());
}

/// Guarda el código para que la ventana lo pueda pedir en cuanto cargue.
fn store_sign_in_code(
    state: &AppState,
    code: &auth::DeviceCode,
    legacy: bool,
) -> CmdResult<SignInCodeEvent> {
    let event = SignInCodeEvent {
        user_code: code.user_code.clone(),
        verification_uri: code.verification_uri.clone(),
        legacy,
        window_opened: false,
    };
    *state.pending_sign_in()? = Some(SignInCodeState {
        user_code: event.user_code.clone(),
        verification_uri: event.verification_uri.clone(),
        legacy,
    });
    Ok(event)
}

/// Intenta mostrar el código en la ventana dedicada. Si no se puede crear,
/// devuelve el evento con `window_opened=false` y la UI principal usa el modal
/// de respaldo.
fn show_sign_in_window(
    app: &AppHandle,
    state: &AppState,
    mut event: SignInCodeEvent,
) -> SignInCodeEvent {
    match open_sign_in_window(app) {
        Ok(_) => {
            event.window_opened = true;
            emit_sign_in_code(app, &event);
        }
        Err(e) => {
            eprintln!("no se pudo abrir la ventana de inicio de sesión: {e}");
            if let Ok(mut pending) = state.pending_sign_in() {
                *pending = None;
            }
        }
    }
    event
}

/// Última cuenta conocida, sin tocar la red (puede estar desactualizada;
/// `play_instance` siempre renueva la sesión real antes de lanzar).
#[tauri::command]
fn get_account(state: State<'_, AppState>) -> CmdResult<Option<AccountView>> {
    let Some(account) = state.auth_store.cached_account() else {
        return Ok(None);
    };
    // El perfil se cachea en disco, pero el refresh token vive en el almacén
    // seguro. Si falta (por ejemplo, una cuenta guardada por 0.1.8 o antes,
    // cuando `keyring` usaba el backend `mock`), no mostrarla como conectada.
    if account.kind == AccountKind::Microsoft {
        match state.auth_store.refresh_token() {
            Ok(Some(_)) => {}
            Ok(None) => {
                let _ = state.auth_store.clear();
                return Ok(None);
            }
            Err(e) => {
                // Un almacén bloqueado no debe impedir arrancar; se mostrará
                // como sin cuenta y el error real aparecerá al iniciar sesión.
                eprintln!("no se pudo leer la sesión guardada: {e}");
                return Ok(None);
            }
        }
    }
    Ok(Some(account.into()))
}

/// Espera al usuario, completa Xbox Live → XSTS → Minecraft y guarda la cuenta
/// con el flujo que corresponde para poder renovarla después.
async fn finish_sign_in(
    app: &AppHandle,
    http: &reqwest::Client,
    ep: &AuthEndpoints,
    client_id: &str,
    code: &auth::DeviceCode,
    cancel: oneshot::Receiver<()>,
    flow: AuthFlow,
) -> Result<Account, CoreError> {
    let tokens = match flow {
        AuthFlow::Legacy => auth::wait_for_legacy_sign_in(http, ep, code, cancel).await?,
        AuthFlow::Azure => auth::wait_for_sign_in(http, ep, client_id, code, cancel).await?,
    };
    let (_session, profile) = match flow {
        AuthFlow::Legacy => auth::chain_from_legacy_msa(http, ep, &tokens.access_token).await?,
        AuthFlow::Azure => auth::chain_from_msa(http, ep, &tokens.access_token).await?,
    };
    let mut account: Account = profile.into();
    account.flow = flow;
    app.state::<AppState>()
        .auth_store
        .save(&tokens.refresh_token, &account)?;
    Ok(account)
}

fn spawn_sign_in(
    app: AppHandle,
    http: reqwest::Client,
    ep: AuthEndpoints,
    client_id: String,
    code: auth::DeviceCode,
    cancel: oneshot::Receiver<()>,
    flow: AuthFlow,
) {
    tauri::async_runtime::spawn(async move {
        let result = finish_sign_in(&app, &http, &ep, &client_id, &code, cancel, flow).await;
        {
            let state = app.state::<AppState>();
            let _ = state.sign_in_cancel().map(|mut slot| *slot = None);
            let _ = state.pending_sign_in().map(|mut pending| *pending = None);
        }
        match result {
            Ok(account) => {
                emit_auth_state(&app, "success", None, Some(account.into()));
                close_sign_in_window(&app);
            }
            Err(e) => {
                let message = e.to_string();
                if message.contains("cancelado") {
                    emit_auth_state(&app, "cancelled", None, None);
                    close_sign_in_window(&app);
                } else {
                    // La ventana dedicada se queda abierta para mostrar el error.
                    emit_auth_state(&app, "error", Some(message), None);
                }
            }
        }
    });
}

/// Pide un código de dispositivo y lo devuelve de inmediato para mostrarlo;
/// la espera y el resto del encadenado Xbox/XSTS/Minecraft siguen en segundo
/// plano y se reportan con el evento `auth-state`.
#[tauri::command]
async fn begin_sign_in(app: AppHandle, state: State<'_, AppState>) -> CmdResult<SignInCodeEvent> {
    if state.sign_in_cancel()?.is_some() {
        return Err(CommandError::new("ya hay un inicio de sesión en curso."));
    }
    let client_id = state.microsoft_client_id()?;
    let ep = AuthEndpoints::live();
    let code = auth::begin(&state.auth_http, &ep, &client_id).await?;
    let event = store_sign_in_code(state.inner(), &code, false)?;

    let (tx, rx) = oneshot::channel();
    *state.sign_in_cancel()? = Some(tx);
    emit_auth_state(&app, "connecting", None, None);
    spawn_sign_in(
        app.clone(),
        state.auth_http.clone(),
        ep,
        client_id,
        code,
        rx,
        AuthFlow::Azure,
    );
    Ok(show_sign_in_window(&app, state.inner(), event))
}

/// Inicio de sesión heredado de `login.live.com`: no requiere Client ID de Azure,
/// pero usa el identificador público del launcher oficial y no es oficial.
#[tauri::command]
async fn begin_legacy_sign_in(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<SignInCodeEvent> {
    if state.sign_in_cancel()?.is_some() {
        return Err(CommandError::new("ya hay un inicio de sesión en curso."));
    }
    let ep = AuthEndpoints::live();
    let code = auth::begin_legacy(&state.auth_http, &ep).await?;
    let event = store_sign_in_code(state.inner(), &code, true)?;

    let (tx, rx) = oneshot::channel();
    *state.sign_in_cancel()? = Some(tx);
    emit_auth_state(&app, "connecting", None, None);
    spawn_sign_in(
        app.clone(),
        state.auth_http.clone(),
        ep,
        String::new(),
        code,
        rx,
        AuthFlow::Legacy,
    );
    Ok(show_sign_in_window(&app, state.inner(), event))
}

/// Código pendiente para la ventana dedicada. La ventana lo pide al cargar para
/// no depender de que el evento llegue antes que el frontend.
#[tauri::command]
fn get_sign_in_code(state: State<'_, AppState>) -> CmdResult<Option<SignInCodeEvent>> {
    let pending = state.pending_sign_in()?;
    Ok(pending.as_ref().map(|code| SignInCodeEvent {
        user_code: code.user_code.clone(),
        verification_uri: code.verification_uri.clone(),
        legacy: code.legacy,
        window_opened: true,
    }))
}

#[tauri::command]
fn cancel_sign_in(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    if let Some(tx) = state.sign_in_cancel()?.take() {
        let _ = tx.send(());
    }
    if let Ok(mut pending) = state.pending_sign_in() {
        *pending = None;
    }
    close_sign_in_window(&app);
    Ok(())
}

/// Inicia sesión con una cuenta local (sin Microsoft/Azure). El nombre y el
/// UUID determinista se guardan en `accounts/account.json`; no se necesita
/// ningún secreto ni conexión.
#[tauri::command]
fn sign_in_offline(state: State<'_, AppState>, username: String) -> CmdResult<AccountView> {
    let account = auth::offline_account(&username)?;
    state.auth_store.save_offline(&account)?;
    Ok(account.into())
}

#[tauri::command]
fn sign_out(state: State<'_, AppState>) -> CmdResult<()> {
    state.auth_store.clear()?;
    Ok(())
}

/// Abre una URL o una carpeta con la aplicación predeterminada del sistema.
///
/// Multiplataforma y sin shell intermedio: en Windows se usa `rundll32`
/// (que no interpreta metacaracteres como `cmd /C start`) y `explorer` para
/// carpetas; así una URL no puede inyectar comandos.
fn open_target(target: &std::ffi::OsStr, is_dir: bool) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        if is_dir {
            std::process::Command::new("explorer.exe")
                .arg(target)
                .spawn()?;
        } else {
            std::process::Command::new("rundll32.exe")
                .arg("url.dll,FileProtocolHandler")
                .arg(target)
                .spawn()?;
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let _ = is_dir;
        std::process::Command::new("open").arg(target).spawn()?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = is_dir;
        std::process::Command::new("xdg-open").arg(target).spawn()?;
        Ok(())
    }
}

/// Abre un enlace en el navegador del sistema. Solo para el enlace de inicio de
/// sesión de Microsoft: esquema https y host en una lista blanca fija, para que
/// el frontend no pueda hacer abrir una URL arbitraria a través de este comando.
#[tauri::command]
fn open_url(url: String) -> CmdResult<()> {
    let parsed =
        reqwest::Url::parse(&url).map_err(|_| CommandError::new("enlace inválido"))?;
    if parsed.scheme() != "https" {
        return Err(CommandError::new("solo se permiten enlaces https"));
    }
    const ALLOWED: &[&str] = &["microsoft.com", "www.microsoft.com", "login.microsoftonline.com"];
    if !ALLOWED.contains(&parsed.host_str().unwrap_or_default()) {
        return Err(CommandError::new("ese enlace no está permitido aquí"));
    }
    open_target(parsed.as_str().as_ref(), false)
        .map_err(|e| CommandError::new(format!("no se pudo abrir el navegador: {e}")))
}

/// Como `open_url`, pero para el enlace de descarga de un manifiesto de
/// actualización configurado por el propio usuario: no hay un host fijo que
/// permitir (la URL la pone un servidor que el usuario eligió), así que solo
/// se exige https.
#[tauri::command]
fn open_https_url(url: String) -> CmdResult<()> {
    let parsed = reqwest::Url::parse(&url).map_err(|_| CommandError::new("enlace inválido"))?;
    if parsed.scheme() != "https" {
        return Err(CommandError::new("solo se permiten enlaces https"));
    }
    open_target(parsed.as_str().as_ref(), false)
        .map_err(|e| CommandError::new(format!("no se pudo abrir el navegador: {e}")))
}

// ───────────────────────────── Instancias ─────────────────────────────

#[tauri::command]
fn list_instances(state: State<'_, AppState>) -> CmdResult<InstanceList> {
    Ok(state.instances.list()?)
}

#[tauri::command]
async fn create_instance(
    state: State<'_, AppState>,
    mut input: NewInstance,
) -> CmdResult<InstanceView> {
    let known = state.installer.version_list().await?;
    if !known.iter().any(|v| v.id == input.minecraft_version.trim()) {
        return Err(CommandError::new(format!(
            "la versión {} no existe en el manifiesto de Mojang",
            input.minecraft_version.trim()
        )));
    }
    if input.loader == Loader::Fabric {
        let versions = state
            .installer
            .fabric_loader_versions(input.minecraft_version.trim())
            .await?;
        let stable = fabric::pick_stable(&versions)?;
        input.loader_version = Some(stable.to_owned());
    }
    Ok(state.instances.create(input)?)
}

#[tauri::command]
async fn update_instance(
    state: State<'_, AppState>,
    id: String,
    patch: UpdateInstance,
) -> CmdResult<InstanceView> {
    let id = InstanceId::parse(&id)?;
    if let Some(choice) = &patch.java {
        let runtimes = java::detect(&state.paths).await;
        java::ensure_known(&runtimes, choice)?;
    }
    Ok(state.instances.update(&id, patch)?)
}

#[tauri::command]
fn delete_instance(state: State<'_, AppState>, id: String) -> CmdResult<Settings> {
    let id = InstanceId::parse(&id)?;
    if state.games()?.contains_key(id.as_str()) {
        return Err(CommandError::new("la instancia está en uso; ciérrala primero"));
    }
    state.instances.delete(&id)?;
    let mut settings = state.settings()?;
    if settings.selected_instance.as_ref() == Some(&id) {
        settings.selected_instance = None;
        settings.save(&state.paths.settings_file())?;
    }
    Ok(settings.clone())
}

#[tauri::command]
fn select_instance(state: State<'_, AppState>, id: Option<String>) -> CmdResult<Settings> {
    let id = id.as_deref().map(InstanceId::parse).transpose()?;
    if let Some(id) = &id {
        state.instances.get(id)?; // debe existir
    }
    let mut settings = state.settings()?;
    settings.selected_instance = id;
    settings.save(&state.paths.settings_file())?;
    Ok(settings.clone())
}

// ───────────────────────────── Mods ─────────────────────────────

fn mods_dir_for(state: &AppState, instance_id: &str) -> CmdResult<std::path::PathBuf> {
    let id = InstanceId::parse(instance_id)?;
    Ok(state.paths.instance_dir(&id).join("mods"))
}

#[tauri::command]
fn list_mods(state: State<'_, AppState>, instance_id: String) -> CmdResult<Vec<ModEntry>> {
    Ok(mods::list(&mods_dir_for(&state, &instance_id)?)?)
}

#[tauri::command]
fn set_mod_enabled(
    state: State<'_, AppState>,
    instance_id: String,
    file_name: String,
    enabled: bool,
) -> CmdResult<String> {
    Ok(mods::set_enabled(&mods_dir_for(&state, &instance_id)?, &file_name, enabled)?)
}

#[tauri::command]
fn delete_mod(state: State<'_, AppState>, instance_id: String, file_name: String) -> CmdResult<()> {
    Ok(mods::delete(&mods_dir_for(&state, &instance_id)?, &file_name)?)
}

/// Un archivo elegido en el selector nativo del navegador (webview): el frontend
/// no tiene la ruta real, así que manda nombre + contenido en base64.
#[derive(Deserialize)]
struct ImportModInput {
    name: String,
    data_base64: String,
}

#[tauri::command]
fn import_mods(
    state: State<'_, AppState>,
    instance_id: String,
    files: Vec<ImportModInput>,
) -> CmdResult<Vec<String>> {
    let mods_dir = mods_dir_for(&state, &instance_id)?;
    if files.len() > MAX_MOD_FILES {
        return Err(CommandError::new(format!(
            "demasiados archivos en una sola importación (máximo {MAX_MOD_FILES})"
        )));
    }
    let mut decoded = Vec::with_capacity(files.len());
    for f in files {
        if f.data_base64.len() > MAX_MOD_B64 {
            return Err(CommandError::new(format!(
                "{}: el archivo es demasiado grande",
                f.name
            )));
        }
        let bytes = BASE64
            .decode(f.data_base64.as_bytes())
            .map_err(|e| CommandError::new(format!("{}: archivo no válido ({e})", f.name)))?;
        decoded.push(mods::ImportFile {
            name: f.name,
            bytes,
        });
    }
    Ok(mods::import(&mods_dir, decoded)?)
}

/// Abre en el explorador de archivos del sistema la carpeta `mods` o `config`
/// de una instancia (nunca una ruta arbitraria: `sub` solo admite esos dos valores).
#[tauri::command]
fn open_instance_folder(
    state: State<'_, AppState>,
    instance_id: String,
    sub: String,
) -> CmdResult<()> {
    if !matches!(sub.as_str(), "" | "mods" | "config") {
        return Err(CommandError::new("carpeta no permitida"));
    }
    let id = InstanceId::parse(&instance_id)?;
    let path = state.paths.instance_dir(&id).join(&sub);
    std::fs::create_dir_all(&path)
        .map_err(|e| CommandError::new(format!("no se pudo preparar la carpeta: {e}")))?;
    open_target(path.as_os_str(), true)
        .map_err(|e| CommandError::new(format!("no se pudo abrir el explorador de archivos: {e}")))
}

// ───────────────────────────── Compartir por código ─────────────────────────────

fn manifest_base_url(state: &AppState) -> CmdResult<String> {
    let url = state
        .settings()?
        .sharing
        .manifest_base_url
        .clone()
        .unwrap_or_default();
    if url.trim().is_empty() {
        return Err(CommandError::new(
            "configura la URL del servicio de manifiestos en Configuración > Compartir antes de usar un código.",
        ));
    }
    Ok(url)
}

#[tauri::command]
async fn fetch_share_manifest(state: State<'_, AppState>, code: String) -> CmdResult<InstanceManifest> {
    let base = manifest_base_url(&state)?;
    Ok(share::fetch_manifest(&state.auth_http, &base, &code).await?)
}

#[derive(Clone, Serialize)]
struct ImportStateEvent {
    /// `success` o `error`.
    state: &'static str,
    instance_id: Option<String>,
    message: Option<String>,
}

/// Instala en segundo plano (misma mecánica que `play_instance`): el progreso
/// llega por el evento `install-progress` ya existente, y el resultado final
/// por `import-state`.
#[tauri::command]
async fn import_from_code(app: AppHandle, state: State<'_, AppState>, code: String) -> CmdResult<()> {
    let base = manifest_base_url(&state)?;
    let manifest = share::fetch_manifest(&state.auth_http, &base, &code).await?;

    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app2.state::<AppState>();
        let progress_id = manifest.code.clone();
        let reporter = {
            let app3 = app2.clone();
            Reporter::new(move |p| {
                let _ = app3.emit(
                    "install-progress",
                    ProgressEvent {
                        id: &progress_id,
                        phase: p.phase,
                        done: p.done,
                        total: p.total,
                    },
                );
            })
        };
        let result = async {
            // Antes de instalar mods/configs se asegura el Java que pide la
            // versión de Minecraft: así una importación deja la instancia
            // realmente lista para jugar, no solo con los archivos.
            let meta = state.installer.version_meta(&manifest.minecraft_version).await?;
            java::ensure_for_meta(&state.paths, &JavaChoice::Auto, &meta, &reporter).await?;
            share::install(&state.paths, &state.instances, &manifest, &reporter).await
        }
        .await;
        let event = match result {
            Ok(id) => ImportStateEvent {
                state: "success",
                instance_id: Some(id.to_string()),
                message: None,
            },
            Err(e) => ImportStateEvent {
                state: "error",
                instance_id: None,
                message: Some(e.to_string()),
            },
        };
        let _ = app2.emit("import-state", event);
    });
    Ok(())
}

/// Escribe una plantilla de manifiesto (sin URLs, a rellenar a mano) dentro de
/// la propia carpeta de la instancia y devuelve la ruta.
#[tauri::command]
fn export_manifest_template(state: State<'_, AppState>, instance_id: String) -> CmdResult<String> {
    let id = InstanceId::parse(&instance_id)?;
    let view = state.instances.get(&id)?;
    let mod_names: Vec<String> = mods::list(&state.paths.instance_dir(&id).join("mods"))?
        .into_iter()
        .filter(|m| m.enabled)
        .map(|m| m.file_name)
        .collect();
    let template = share::export_template(
        id.as_str(),
        &view.instance.name,
        &view.instance.minecraft_version,
        view.instance.loader,
        view.instance.loader_version.as_deref(),
        &mod_names,
    );
    let json = serde_json::to_string_pretty(&template)
        .map_err(|e| CommandError::new(format!("no se pudo generar el manifiesto: {e}")))?;
    let dest = state.paths.instance_dir(&id).join("manifest-template.json");
    std::fs::write(&dest, json)
        .map_err(|e| CommandError::new(format!("no se pudo escribir el manifiesto: {e}")))?;
    Ok(dest.to_string_lossy().into_owned())
}

// ───────────────────────────── Fondo de instancia ─────────────────────────────

#[derive(Serialize)]
struct BackgroundView {
    mime: String,
    data_base64: String,
}

#[tauri::command]
fn get_instance_background(state: State<'_, AppState>, instance_id: String) -> CmdResult<Option<BackgroundView>> {
    let id = InstanceId::parse(&instance_id)?;
    let dir = state.paths.instance_dir(&id);
    Ok(background::load(&dir)?.map(|(mime, bytes)| BackgroundView {
        mime,
        data_base64: BASE64.encode(bytes),
    }))
}

#[tauri::command]
fn set_instance_background(
    state: State<'_, AppState>,
    instance_id: String,
    mime: String,
    data_base64: String,
) -> CmdResult<()> {
    let id = InstanceId::parse(&instance_id)?;
    if data_base64.len() > MAX_BACKGROUND_B64 {
        return Err(CommandError::new(
            "la imagen supera el tamaño máximo admitido (15 MB)",
        ));
    }
    let bytes = BASE64
        .decode(data_base64.as_bytes())
        .map_err(|e| CommandError::new(format!("imagen no válida: {e}")))?;
    Ok(background::save(&state.paths.instance_dir(&id), &mime, &bytes)?)
}

#[tauri::command]
fn clear_instance_background(state: State<'_, AppState>, instance_id: String) -> CmdResult<()> {
    let id = InstanceId::parse(&instance_id)?;
    Ok(background::clear(&state.paths.instance_dir(&id))?)
}

// ───────────────────────────── Fondo e icono del launcher ─────────────────────────────

#[tauri::command]
fn get_launcher_background(state: State<'_, AppState>) -> CmdResult<Option<BackgroundView>> {
    Ok(background::load_launcher_background(state.paths.config_dir())?.map(|(mime, bytes)| {
        BackgroundView {
            mime,
            data_base64: BASE64.encode(bytes),
        }
    }))
}

#[tauri::command]
fn set_launcher_background(
    state: State<'_, AppState>,
    mime: String,
    data_base64: String,
) -> CmdResult<()> {
    if data_base64.len() > MAX_BACKGROUND_B64 {
        return Err(CommandError::new(
            "la imagen supera el tamaño máximo admitido (15 MB)",
        ));
    }
    let bytes = BASE64
        .decode(data_base64.as_bytes())
        .map_err(|e| CommandError::new(format!("imagen no válida: {e}")))?;
    Ok(background::save_launcher_background(
        state.paths.config_dir(),
        &mime,
        &bytes,
    )?)
}

#[tauri::command]
fn clear_launcher_background(state: State<'_, AppState>) -> CmdResult<()> {
    Ok(background::clear_launcher_background(state.paths.config_dir())?)
}

#[tauri::command]
fn get_launcher_icon(state: State<'_, AppState>) -> CmdResult<Option<BackgroundView>> {
    Ok(background::load_launcher_icon(state.paths.config_dir())?.map(|(mime, bytes)| {
        BackgroundView {
            mime,
            data_base64: BASE64.encode(bytes),
        }
    }))
}

#[tauri::command]
fn set_launcher_icon(
    app: AppHandle,
    state: State<'_, AppState>,
    mime: String,
    data_base64: String,
) -> CmdResult<()> {
    if mime != "image/png" {
        return Err(CommandError::new("el icono debe ser un PNG cuadrado"));
    }
    if data_base64.len() > MAX_ICON_B64 {
        return Err(CommandError::new(
            "el icono supera el tamaño máximo admitido (4 MB)",
        ));
    }
    let bytes = BASE64
        .decode(data_base64.as_bytes())
        .map_err(|e| CommandError::new(format!("icono no válido: {e}")))?;
    let (width, height) = background::png_dimensions(&bytes)
        .ok_or_else(|| CommandError::new("icono no válido: no es un PNG"))?;
    if width != height {
        return Err(CommandError::new("el icono debe ser un PNG cuadrado"));
    }
    if width > 2048 || height > 2048 {
        return Err(CommandError::new(
            "el icono no puede superar 2048×2048 píxeles",
        ));
    }
    let icon = tauri::image::Image::from_bytes(&bytes)
        .map_err(|e| CommandError::new(format!("icono no válido: {e}")))?;
    background::save_launcher_icon(state.paths.config_dir(), &bytes)?;
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_icon(icon)
            .map_err(|e| CommandError::new(format!("no se pudo aplicar el icono: {e}")))?;
    }
    Ok(())
}

#[tauri::command]
fn clear_launcher_icon(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    background::clear_launcher_icon(state.paths.config_dir())?;
    if let Some(window) = app.get_webview_window("main") {
        if let Some(icon) = app.default_window_icon().cloned() {
            let _ = window.set_icon(icon);
        }
    }
    Ok(())
}

// ───────────────────────────── Jugar ─────────────────────────────

#[derive(Clone, Serialize)]
struct GameStateEvent {
    id: String,
    /// `preparing`, `running`, `stopped`, `error`.
    state: &'static str,
    message: Option<String>,
    exit_code: Option<i32>,
}

#[derive(Clone, Serialize)]
struct ProgressEvent<'a> {
    id: &'a str,
    phase: String,
    done: u64,
    total: u64,
}

#[derive(Clone, Serialize)]
struct LogEvent<'a> {
    id: &'a str,
    stream: &'static str,
    text: String,
}

fn emit_state(app: &AppHandle, id: &str, state: &'static str, message: Option<String>, code: Option<i32>) {
    let _ = app.emit(
        "game-state",
        GameStateEvent {
            id: id.to_owned(),
            state,
            message,
            exit_code: code,
        },
    );
}

/// Prepara (descarga/verifica) y lanza. Devuelve el código de salida del juego.
async fn prepare_and_run(app: &AppHandle, id: &InstanceId) -> Result<Option<i32>, CoreError> {
    let state = app.state::<AppState>();
    let view = state.instances.get(id)?;
    let instance = view.instance;
    let settings = state
        .settings
        .lock()
        .map_err(|_| CoreError::Launch("estado interno corrupto".into()))?
        .clone();

    let id_str = id.to_string();
    let progress_app = app.clone();
    let progress_id = id_str.clone();
    let reporter = Reporter::new(move |p| {
        let _ = progress_app.emit(
            "install-progress",
            ProgressEvent {
                id: &progress_id,
                phase: p.phase,
                done: p.done,
                total: p.total,
            },
        );
    });

    let prepared = state
        .installer
        .prepare(
            &instance.minecraft_version,
            instance.loader,
            instance.loader_version.as_deref(),
            &reporter,
        )
        .await?;

    let runtime = java::ensure_for_meta(&state.paths, &instance.java, &prepared.meta, &reporter)
        .await?;

    // La cuenta local no toca la red. La de Microsoft renueva su sesión real
    // antes de lanzar; sin cuenta guardada se mantiene el modo demo oficial.
    let launch_auth = match state.auth_store.cached_account() {
        Some(account) if account.kind == AccountKind::Offline => LaunchAuth::offline(&account),
        Some(_) => {
            let client_id = settings.account.microsoft_client_id.clone().unwrap_or_default();
            let ep = AuthEndpoints::live();
            match auth::resume(&state.auth_http, &ep, &client_id, &state.auth_store).await? {
                Some(signed_in) => {
                    LaunchAuth::from_session(&signed_in.account, &signed_in.session)
                }
                None => {
                    // Hay un perfil de Microsoft cacheado pero no un refresh
                    // token: lanzar en modo demo haría creer que la cuenta se
                    // perdió sin explicación. Mejor pedir que se inicie sesión.
                    return Err(CoreError::Auth(
                        "tu sesión de Microsoft no está disponible en este PC; \
                         inicia sesión de nuevo."
                            .into(),
                    ));
                }
            }
        }
        None => LaunchAuth::demo(),
    };

    let game_dir = state.paths.instance_dir(id);
    if prepared.map_to_resources {
        copy_assets(
            &prepared.asset_index,
            &state.paths.assets_dir().join("objects"),
            &game_dir.join("resources"),
        )
        .await?;
    }

    let libraries_dir = state.paths.libraries_dir();
    let command = launcher::build_command(&LaunchInput {
        prepared: &prepared,
        java: &runtime.path,
        game_dir: &game_dir,
        libraries_dir: &libraries_dir,
        memory: instance.memory,
        user_jvm_args: &instance.jvm_args,
        auth: &launch_auth,
        screen: Screen {
            width: settings.minecraft.width,
            height: settings.minecraft.height,
            fullscreen: settings.minecraft.fullscreen,
        },
    })?;

    state.instances.touch_last_played(id)?;

    let (kill_tx, kill_rx) = oneshot::channel();
    {
        let mut games = state
            .games
            .lock()
            .map_err(|_| CoreError::Launch("estado interno corrupto".into()))?;
        games.insert(id_str.clone(), GameSlot::Running(kill_tx));
    }
    emit_state(app, &id_str, "running", None, None);

    let log_app = app.clone();
    let log_id = id_str.clone();
    let sink: Arc<dyn Fn(launcher::LogLine) + Send + Sync> = Arc::new(move |line| {
        let _ = log_app.emit(
            "game-log",
            LogEvent {
                id: &log_id,
                stream: line.stream,
                text: line.text,
            },
        );
    });
    let started_at = std::time::Instant::now();
    let outcome = launcher::run_game(
        command,
        sink,
        Some(game_dir.join("logs").join("nebula-latest.log")),
        kill_rx,
    )
    .await;
    // Se suma el tiempo jugado aunque el juego haya terminado con error o se
    // haya forzado el cierre: la sesión ocurrió de verdad de todos modos.
    state.instances.add_playtime(id, started_at.elapsed().as_secs())?;
    outcome
}

/// Inicia preparación + lanzamiento en segundo plano. El progreso llega por eventos.
#[tauri::command]
fn play_instance(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let id = InstanceId::parse(&id)?;
    {
        let mut games = state.games()?;
        if games.contains_key(id.as_str()) {
            return Err(CommandError::new("esta instancia ya se está preparando o ejecutando"));
        }
        games.insert(id.to_string(), GameSlot::Preparing);
    }
    emit_state(&app, id.as_str(), "preparing", None, None);

    tauri::async_runtime::spawn(async move {
        let result = prepare_and_run(&app, &id).await;
        if let Ok(mut games) = app.state::<AppState>().games.lock() {
            games.remove(id.as_str());
        }
        match result {
            Ok(code) => emit_state(&app, id.as_str(), "stopped", None, code),
            Err(e) => emit_state(&app, id.as_str(), "error", Some(e.to_string()), None),
        }
    });
    Ok(())
}

#[tauri::command]
fn stop_instance(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let id = InstanceId::parse(&id)?;
    let mut games = state.games()?;
    match games.remove(id.as_str()) {
        Some(GameSlot::Running(kill)) => {
            let _ = kill.send(());
            // Se vuelve a registrar como «en parada» hasta que el proceso termine.
            games.insert(id.to_string(), GameSlot::Preparing);
            Ok(())
        }
        Some(slot) => {
            games.insert(id.to_string(), slot);
            Err(CommandError::new("todavía se está preparando; espera a que arranque"))
        }
        None => Err(CommandError::new("la instancia no se está ejecutando")),
    }
}

#[tauri::command]
fn game_states(state: State<'_, AppState>) -> CmdResult<HashMap<String, &'static str>> {
    Ok(state
        .games()?
        .iter()
        .map(|(id, slot)| {
            (
                id.clone(),
                match slot {
                    GameSlot::Preparing => "preparing",
                    GameSlot::Running(_) => "running",
                },
            )
        })
        .collect())
}


#[derive(Debug, Serialize, Clone)]
struct NebulaUpdateInfo {
    version: String,
    current_version: String,
    notes: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
struct NebulaUpdateStatus {
    available: bool,
    update: Option<NebulaUpdateInfo>,
}

/// Progreso de descarga de una actualización (para la barra del diálogo).
#[derive(Debug, Serialize, Clone)]
struct UpdateProgressEvent {
    downloaded: u64,
    total: Option<u64>,
}

#[tauri::command]
async fn check_and_prepare_update(app: AppHandle) -> CmdResult<NebulaUpdateStatus> {
    let updater = match app.updater() {
        Ok(u) => u,
        Err(_) => {
            return Ok(NebulaUpdateStatus {
                available: false,
                update: None,
            });
        }
    };
    match updater.check().await {
        Ok(Some(update)) => Ok(NebulaUpdateStatus {
            available: true,
            update: Some(NebulaUpdateInfo {
                version: update.version.clone(),
                current_version: update.current_version.clone(),
                notes: update.body.clone(),
            }),
        }),
        Ok(None) => Ok(NebulaUpdateStatus {
            available: false,
            update: None,
        }),
        Err(e) => Err(CommandError::new(format!("Error al comprobar actualizaciones: {e}"))),
    }
}

#[tauri::command]
async fn install_update(app: AppHandle) -> CmdResult<()> {
    let updater = app
        .updater()
        .map_err(|e| CommandError::new(format!("Actualizador no disponible: {e}")))?;
    let update = updater
        .check()
        .await
        .map_err(|e| CommandError::new(format!("Error al comprobar actualización: {e}")))?
        .ok_or_else(|| CommandError::new("No hay actualizaciones disponibles"))?;

    // Se emite como mucho un evento por MB para no saturar el IPC con un
    // AppImage de ~108 MB.
    let progress_app = app.clone();
    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;
    update
        .download_and_install(
            move |chunk, total| {
                downloaded = downloaded.saturating_add(chunk as u64);
                if downloaded.saturating_sub(last_emit) >= 1024 * 1024 {
                    last_emit = downloaded;
                    let _ = progress_app.emit(
                        "update-progress",
                        UpdateProgressEvent { downloaded, total },
                    );
                }
            },
            || {},
        )
        .await
        .map_err(|e| CommandError::new(format!("Error al instalar actualización: {e}")))?;

    // Linux: el plugin sustituye el AppImage pero no lo relanza. Hay que abrir
    // la ruta real del AppImage ($APPIMAGE), no `current_exe()`, que apunta al
    // montaje temporal de la versión vieja.
    #[cfg(target_os = "linux")]
    {
        if let Ok(appimage) = std::env::var("APPIMAGE") {
            std::process::Command::new(&appimage).spawn().map_err(|e| {
                CommandError::new(format!(
                    "la actualización se instaló, pero no se pudo reiniciar: {e}"
                ))
            })?;
            app.exit(0);
        }
    }

    // macOS: el bundle se reemplaza en su ruta y `restart` relanza esa ruta.
    #[cfg(target_os = "macos")]
    {
        return app.restart();
    }

    // Windows: el instalador lanzado por el plugin cierra y reinicia la app.
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .on_window_event(|window, event| {
            // Si el usuario cierra la ventana de inicio de sesión, se cancela
            // el flujo para que la ventana principal no se quede esperando.
            if window.label() == "signin" {
                if let tauri::WindowEvent::Destroyed = event {
                    let app = window.app_handle();
                    if let Some(state) = app.try_state::<AppState>() {
                        if let Ok(mut slot) = state.sign_in_cancel() {
                            if let Some(tx) = slot.take() {
                                let _ = tx.send(());
                            }
                        }
                        if let Ok(mut pending) = state.pending_sign_in() {
                            *pending = None;
                        }
                    }
                }
            }
        })
        .setup(|app| {
            let boot = NebulaPaths::from_system()?;
            let settings = Settings::load(&boot.settings_file())?;
            let paths = match &settings.data_dir {
                Some(dir) => boot.with_data_dir(dir)?,
                None => boot,
            };
            paths.ensure_layout()?;
            let config_dir = paths.config_dir().to_path_buf();
            app.manage(AppState {
                instances: InstanceStore::new(paths.clone()),
                installer: Installer::new(paths.clone())?,
                auth_store: AccountStore::new(&paths),
                auth_http: auth::http_client()?,
                paths,
                settings: Mutex::new(settings),
                games: Mutex::new(HashMap::new()),
                sign_in_cancel: Mutex::new(None),
                sign_in_code: Mutex::new(None),
            });
            // `bundle.icon` empieza por `icons/icon.png` (512×512), así que este
            // es el icono de Halloween que Tauri incrusta para la ventana. Si el
            // usuario guardó uno propio, se aplica al arrancar.
            if let Some(window) = app.get_webview_window("main") {
                let custom = background::load_launcher_icon(&config_dir)
                    .ok()
                    .flatten()
                    .and_then(|(_, bytes)| tauri::image::Image::from_bytes(&bytes).ok());
                if let Some(icon) = custom {
                    let _ = window.set_icon(icon);
                } else if let Some(icon) = app.default_window_icon().cloned() {
                    let _ = window.set_icon(icon);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            get_settings,
            save_settings,
            get_account,
            get_sign_in_code,
            begin_sign_in,
            begin_legacy_sign_in,
            sign_in_offline,
            cancel_sign_in,
            sign_out,
            open_url,
            open_https_url,
            check_for_updates,
            check_and_prepare_update,
            install_update,
            list_java_runtimes,
            list_minecraft_versions,
            list_instances,
            create_instance,
            update_instance,
            delete_instance,
            select_instance,
            list_mods,
            set_mod_enabled,
            delete_mod,
            import_mods,
            open_instance_folder,
            get_instance_background,
            set_instance_background,
            clear_instance_background,
            get_launcher_background,
            set_launcher_background,
            clear_launcher_background,
            get_launcher_icon,
            set_launcher_icon,
            clear_launcher_icon,
            fetch_share_manifest,
            import_from_code,
            export_manifest_template,
            play_instance,
            stop_instance,
            game_states,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            eprintln!("error al iniciar Nebula: {e}");
            std::process::exit(1);
        });
}
