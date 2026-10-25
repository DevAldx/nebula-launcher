//! Construcción de la línea de comandos de Minecraft y ejecución del proceso.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde::Serialize;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader},
    process::Command,
    sync::{oneshot, Mutex},
};

use crate::{
    auth::{Account, McSession},
    error::CoreError,
    instances::Memory,
    minecraft::{
        install::PreparedVersion,
        meta::{rules_allow, Arg, ArgValue},
    },
    Result,
};

/// Credenciales con las que se lanza el juego.
///
/// Fase 2: solo existe el modo demo oficial de Minecraft (`--demo`), sin cuenta.
/// La Fase 3 añade `LaunchAuth::authenticated(...)` con el perfil de Microsoft.
#[derive(Debug, Clone)]
pub struct LaunchAuth {
    pub player_name: String,
    /// UUID sin guiones.
    pub uuid: String,
    pub access_token: String,
    pub user_type: String,
    pub demo: bool,
}

impl LaunchAuth {
    pub fn demo() -> Self {
        Self {
            player_name: "Player".into(),
            uuid: "00000000-0000-0000-0000-000000000000".into(),
            access_token: "0".into(),
            user_type: "legacy".into(),
            demo: true,
        }
    }

    /// Sesión real, a partir de una cuenta de Microsoft ya autenticada (Fase 3).
    pub fn from_session(account: &Account, session: &McSession) -> Self {
        Self {
            player_name: account.username.clone(),
            uuid: account.uuid.clone(),
            access_token: session.access_token.clone(),
            user_type: "msa".into(),
            demo: false,
        }
    }

    /// Cuenta local para servidores `online-mode=false`: sin token real, el
    /// servidor solo comprueba el nombre y el UUID derivado del mismo.
    pub fn offline(account: &Account) -> Self {
        Self {
            player_name: account.username.clone(),
            uuid: account.uuid.replace('-', ""),
            access_token: "0".into(),
            user_type: "legacy".into(),
            demo: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Screen {
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
}

#[derive(Debug, Clone)]
pub struct LaunchCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    /// Textos que nunca deben aparecer en logs (tokens).
    pub secrets: Vec<String>,
}

pub struct LaunchInput<'a> {
    pub prepared: &'a PreparedVersion,
    pub java: &'a Path,
    pub game_dir: &'a Path,
    pub libraries_dir: &'a Path,
    pub memory: Memory,
    pub user_jvm_args: &'a [String],
    pub auth: &'a LaunchAuth,
    pub screen: Screen,
}

/// Windows separa las entradas del classpath con `;`; el resto, con `:`.
const CLASSPATH_SEPARATOR: &str = if cfg!(windows) { ";" } else { ":" };

fn substitute(template: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = template.to_owned();
    for (key, value) in vars {
        out = out.replace(&format!("${{{key}}}"), value);
    }
    out
}

fn expand(args: &[Arg], features: &HashSet<&str>, vars: &HashMap<&str, String>) -> Vec<String> {
    let mut out = Vec::new();
    for arg in args {
        match arg {
            Arg::Plain(s) => out.push(substitute(s, vars)),
            Arg::Conditional { rules, value } => {
                if !rules_allow(rules, features) {
                    continue;
                }
                match value {
                    ArgValue::One(s) => out.push(substitute(s, vars)),
                    ArgValue::Many(list) => out.extend(list.iter().map(|s| substitute(s, vars))),
                }
            }
        }
    }
    out
}

pub fn build_command(input: &LaunchInput<'_>) -> Result<LaunchCommand> {
    let p = input.prepared;
    let path_str = |p: &Path| p.to_string_lossy().into_owned();

    let mut classpath: Vec<String> = p.classpath_libs.iter().map(|x| path_str(x)).collect();
    classpath.push(path_str(&p.client_jar));

    let mut features: HashSet<&str> = HashSet::new();
    features.insert("has_custom_resolution");
    if input.auth.demo {
        features.insert("is_demo_user");
    }

    let vars: HashMap<&str, String> = HashMap::from([
        ("auth_player_name", input.auth.player_name.clone()),
        ("version_name", p.meta.id.clone()),
        ("game_directory", path_str(input.game_dir)),
        ("assets_root", path_str(&p.assets_root)),
        ("assets_index_name", p.assets_index_name.clone()),
        ("auth_uuid", input.auth.uuid.clone()),
        ("auth_access_token", input.auth.access_token.clone()),
        ("auth_session", format!("token:{}:{}", input.auth.access_token, input.auth.uuid)),
        ("clientid", String::new()),
        ("auth_xuid", String::new()),
        ("user_type", input.auth.user_type.clone()),
        ("user_properties", "{}".into()),
        ("version_type", p.meta.kind.clone()),
        ("resolution_width", input.screen.width.to_string()),
        ("resolution_height", input.screen.height.to_string()),
        ("natives_directory", path_str(&p.natives_dir)),
        ("launcher_name", "Nebula".into()),
        ("launcher_version", env!("CARGO_PKG_VERSION").into()),
        ("classpath", classpath.join(CLASSPATH_SEPARATOR)),
        ("classpath_separator", CLASSPATH_SEPARATOR.into()),
        ("library_directory", path_str(input.libraries_dir)),
        ("game_assets", path_str(&p.game_assets_dir)),
    ]);

    let mut args = vec![
        format!("-Xms{}M", input.memory.min_mb),
        format!("-Xmx{}M", input.memory.max_mb),
    ];
    args.extend(input.user_jvm_args.iter().cloned());

    match p.meta.arguments.as_ref().filter(|a| !a.jvm.is_empty()) {
        Some(a) => args.extend(expand(&a.jvm, &features, &vars)),
        // Formato antiguo: el launcher oficial pasa estos dos.
        None => args.extend([
            format!("-Djava.library.path={}", path_str(&p.natives_dir)),
            "-cp".into(),
            classpath.join(CLASSPATH_SEPARATOR),
        ]),
    }
    if let Some(logging) = &p.logging_arg {
        args.push(logging.clone());
    }
    args.push(p.meta.main_class.clone());

    match (&p.meta.arguments, &p.meta.minecraft_arguments) {
        (Some(a), _) if !a.game.is_empty() => args.extend(expand(&a.game, &features, &vars)),
        (_, Some(legacy)) => {
            args.extend(legacy.split_whitespace().map(|s| substitute(s, &vars)));
            if input.auth.demo {
                args.push("--demo".into());
            }
            if !args.iter().any(|a| a == "--width") {
                args.extend([
                    "--width".into(),
                    input.screen.width.to_string(),
                    "--height".into(),
                    input.screen.height.to_string(),
                ]);
            }
        }
        _ => {
            return Err(CoreError::Launch(format!(
                "la versión {} no define argumentos de juego",
                p.meta.id
            )))
        }
    }
    if input.screen.fullscreen {
        args.push("--fullscreen".into());
    }

    let secrets = if input.auth.access_token.len() >= 8 {
        vec![input.auth.access_token.clone()]
    } else {
        vec![]
    };
    Ok(LaunchCommand {
        program: input.java.to_path_buf(),
        args,
        cwd: input.game_dir.to_path_buf(),
        secrets,
    })
}

// ───────────────────────────── Ejecución ─────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    /// `stdout` o `stderr`.
    pub stream: &'static str,
    pub text: String,
}

pub fn redact(line: &str, secrets: &[String]) -> String {
    secrets
        .iter()
        .filter(|s| !s.is_empty())
        .fold(line.to_owned(), |acc, s| acc.replace(s.as_str(), "***"))
}

type LineSink = Arc<dyn Fn(LogLine) + Send + Sync>;

async fn pump<R: AsyncRead + Unpin>(
    reader: R,
    stream: &'static str,
    secrets: Arc<Vec<String>>,
    sink: LineSink,
    file: Arc<Mutex<Option<tokio::fs::File>>>,
) {
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let raw = String::from_utf8_lossy(&buf);
                let text = redact(raw.trim_end_matches(['\n', '\r']), &secrets);
                if let Some(f) = file.lock().await.as_mut() {
                    let _ = f.write_all(format!("[{stream}] {text}\n").as_bytes()).await;
                }
                sink(LogLine { stream, text });
            }
        }
    }
}

/// Ejecuta el juego hasta que termine (o `kill` se dispare). Devuelve el código de salida.
/// Captura stdout/stderr línea a línea, con tokens censurados, y opcionalmente
/// los guarda en `log_file`.
pub async fn run_game(
    cmd: LaunchCommand,
    sink: LineSink,
    log_file: Option<PathBuf>,
    kill: oneshot::Receiver<()>,
) -> Result<Option<i32>> {
    let mut child = Command::new(&cmd.program)
        .args(&cmd.args)
        .current_dir(&cmd.cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| CoreError::Launch(format!("no se pudo iniciar Java: {e}")))?;

    let file = match log_file {
        Some(path) => tokio::fs::File::create(&path).await.ok(),
        None => None,
    };
    let file = Arc::new(Mutex::new(file));
    let secrets = Arc::new(cmd.secrets);

    let mut readers = Vec::new();
    if let Some(out) = child.stdout.take() {
        readers.push(tokio::spawn(pump(out, "stdout", secrets.clone(), sink.clone(), file.clone())));
    }
    if let Some(err) = child.stderr.take() {
        readers.push(tokio::spawn(pump(err, "stderr", secrets.clone(), sink.clone(), file.clone())));
    }

    let status = tokio::select! {
        s = child.wait() => s,
        requested = kill => {
            if requested.is_ok() {
                let _ = child.kill().await;
            }
            child.wait().await
        }
    }
    .map_err(|e| CoreError::Launch(e.to_string()))?;

    for reader in readers {
        let _ = tokio::time::timeout(Duration::from_secs(2), reader).await;
    }
    Ok(status.code())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minecraft::{
        install::PreparedVersion,
        meta::{fixtures::{LEGACY, MODERN}, AssetIndex, VersionMeta},
    };
    use std::sync::Mutex as StdMutex;

    fn prepared(json: &str, logging: Option<&str>) -> PreparedVersion {
        let meta: VersionMeta = serde_json::from_str(json).expect("fixture");
        PreparedVersion {
            client_jar: PathBuf::from("/v/client.jar"),
            classpath_libs: vec![PathBuf::from("/l/a.jar"), PathBuf::from("/l/b.jar")],
            natives_dir: PathBuf::from("/v/natives"),
            assets_root: PathBuf::from("/assets"),
            assets_index_name: meta.asset_index.id.clone(),
            game_assets_dir: PathBuf::from("/assets"),
            map_to_resources: false,
            asset_index: AssetIndex {
                objects: HashMap::new(),
                r#virtual: false,
                map_to_resources: false,
            },
            logging_arg: logging.map(str::to_owned),
            meta,
        }
    }

    fn build(p: &PreparedVersion, auth: &LaunchAuth, fullscreen: bool) -> Vec<String> {
        let input = LaunchInput {
            prepared: p,
            java: Path::new("/jvm/bin/java"),
            game_dir: Path::new("/inst/NBL-AAAAAA"),
            libraries_dir: Path::new("/libs"),
            memory: Memory { min_mb: 1024, max_mb: 2048 },
            user_jvm_args: &["-XX:+UseG1GC".to_owned()],
            auth,
            screen: Screen { width: 854, height: 480, fullscreen },
        };
        build_command(&input).expect("command").args
    }

    fn value_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(String::as_str)
    }

    #[test]
    fn modern_command_is_complete_and_ordered() {
        let p = prepared(MODERN, Some("-Dlog4j.configurationFile=/assets/log_configs/c.xml"));
        let args = build(&p, &LaunchAuth::demo(), false);
        assert_eq!(&args[..3], ["-Xms1024M", "-Xmx2048M", "-XX:+UseG1GC"]);
        assert!(args.contains(&"-Djava.library.path=/v/natives".to_owned()));
        assert_eq!(value_after(&args, "-cp"), Some("/l/a.jar:/l/b.jar:/v/client.jar"));
        // Regla de macOS/Windows: no deben aparecer.
        assert!(!args.iter().any(|a| a.contains("XstartOnFirstThread") || a.contains("HeapDumpPath")));
        let main = args.iter().position(|a| a == "net.minecraft.client.main.Main").expect("main");
        assert!(args.iter().position(|a| a.starts_with("-Dlog4j")).expect("log") < main);
        assert!(args.iter().position(|a| a == "-cp").expect("cp") < main);
        assert_eq!(value_after(&args, "--gameDir"), Some("/inst/NBL-AAAAAA"));
        assert_eq!(value_after(&args, "--version"), Some("1.21.1"));
        assert_eq!(value_after(&args, "--assetIndex"), Some("17"));
        assert_eq!(value_after(&args, "--width"), Some("854"));
        assert!(args.contains(&"--demo".to_owned()));
        assert!(args.iter().all(|a| !a.contains("${")), "placeholders sin resolver: {args:?}");
    }

    #[test]
    fn non_demo_modern_has_no_demo_flag_and_fullscreen_is_added() {
        let p = prepared(MODERN, None);
        let mut auth = LaunchAuth::demo();
        auth.demo = false;
        let args = build(&p, &auth, true);
        assert!(!args.contains(&"--demo".to_owned()));
        assert_eq!(args.last().map(String::as_str), Some("--fullscreen"));
    }

    #[test]
    fn legacy_command_uses_default_jvm_args_and_minecraft_arguments() {
        let p = prepared(LEGACY, None);
        let args = build(&p, &LaunchAuth::demo(), false);
        assert_eq!(value_after(&args, "-cp"), Some("/l/a.jar:/l/b.jar:/v/client.jar"));
        assert!(args.contains(&"-Djava.library.path=/v/natives".to_owned()));
        assert_eq!(value_after(&args, "--username"), Some("Player"));
        assert_eq!(value_after(&args, "--assetsDir"), Some("/assets"));
        assert_eq!(value_after(&args, "--userProperties"), Some("{}"));
        assert!(args.contains(&"--demo".to_owned()));
        assert_eq!(value_after(&args, "--height"), Some("480"));
        assert!(args.iter().all(|a| !a.contains("${")), "{args:?}");
    }

    #[test]
    fn redaction_hides_secrets() {
        let secrets = vec!["s3cr3t-token".to_owned()];
        assert_eq!(redact("token=s3cr3t-token fin", &secrets), "token=*** fin");
        assert_eq!(redact("sin secretos", &secrets), "sin secretos");
    }

    #[tokio::test]
    async fn run_game_captures_output_redacts_and_reports_exit_code() -> Result<()> {
        let lines = Arc::new(StdMutex::new(Vec::<LogLine>::new()));
        let sink_lines = lines.clone();
        let sink: LineSink = Arc::new(move |l| {
            if let Ok(mut g) = sink_lines.lock() {
                g.push(l);
            }
        });
        let cmd = LaunchCommand {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "echo salida; echo error >&2; echo tok-12345678; exit 3".into()],
            cwd: std::env::temp_dir(),
            secrets: vec!["tok-12345678".into()],
        };
        let (_tx, rx) = oneshot::channel();
        let code = run_game(cmd, sink, None, rx).await?;
        assert_eq!(code, Some(3));
        let got = lines.lock().map(|g| g.clone()).unwrap_or_default();
        let texts: Vec<_> = got.iter().map(|l| (l.stream, l.text.as_str())).collect();
        assert!(texts.contains(&("stdout", "salida")));
        assert!(texts.contains(&("stderr", "error")));
        assert!(texts.contains(&("stdout", "***")));
        assert!(!texts.iter().any(|(_, t)| t.contains("tok-12345678")));
        Ok(())
    }

    #[tokio::test]
    async fn run_game_can_be_killed() -> Result<()> {
        let sink: LineSink = Arc::new(|_| {});
        let cmd = LaunchCommand {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "sleep 30".into()],
            cwd: std::env::temp_dir(),
            secrets: vec![],
        };
        let (tx, rx) = oneshot::channel();
        let task = tokio::spawn(run_game(cmd, sink, None, rx));
        tokio::time::sleep(Duration::from_millis(200)).await;
        let _ = tx.send(());
        let res = tokio::time::timeout(Duration::from_secs(5), task).await;
        assert!(res.is_ok(), "el proceso debía terminar tras kill");
        Ok(())
    }
}
