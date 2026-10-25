//! Detección de Java instalado y elección del runtime adecuado para cada versión.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Serialize;
use tokio::process::Command;

use crate::{
    error::CoreError,
    instances::JavaChoice,
    minecraft::{meta::VersionMeta, Reporter},
    NebulaPaths, Result,
};

mod install;

pub use install::{install_dir, install_major, JAVA_HOSTS};

/// Versiones anteriores a esta fecha (1.13) solo funcionan de forma fiable con Java 8.
const JAVA8_ONLY_BEFORE: &str = "2018-07-18";

/// En Windows el ejecutable se llama `java.exe`.
pub fn java_binary(root: &Path) -> PathBuf {
    root.join("bin")
        .join(if cfg!(windows) { "java.exe" } else { "java" })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JavaRuntime {
    pub path: PathBuf,
    pub major: u32,
    pub version: String,
}

/// Interpreta la salida de `java -version`:
/// `openjdk version "1.8.0_392"` → (8, "1.8.0_392"); `"17.0.9"` → (17, …); `"21"` → (21, …).
pub fn parse_java_version(output: &str) -> Option<(u32, String)> {
    let start = output.find("version \"")? + "version \"".len();
    let version = output[start..].split('"').next()?.to_owned();
    let mut parts = version.split(|c: char| !c.is_ascii_digit());
    let first: u32 = parts.next()?.parse().ok()?;
    let major = if first == 1 {
        parts.next()?.parse().ok()?
    } else {
        first
    };
    Some((major, version))
}

fn candidate_paths(paths: &NebulaPaths) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = vec![paths.java_dir()];

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        roots.extend([
            PathBuf::from("/usr/lib/jvm"),
            PathBuf::from("/usr/lib64/jvm"),
            PathBuf::from("/usr/java"),
        ]);
    }

    #[cfg(target_os = "windows")]
    {
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(base) = std::env::var_os(var).map(PathBuf::from) {
                roots.push(base.join("Java"));
                roots.push(base.join("Eclipse Adoptium"));
                roots.push(base.join("Eclipse Foundation"));
                roots.push(base.join("Microsoft"));
                roots.push(base.join("Zulu"));
                roots.push(base.join("BellSoft"));
                roots.push(base.join("Amazon Corretto"));
            }
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            roots.push(local.join("Programs").join("Eclipse Adoptium"));
            roots.push(local.join("Programs").join("Microsoft"));
        }
    }

    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        roots.push(home.join(".jdks"));
        roots.push(home.join(".sdkman/candidates/java"));
    }

    let mut out = Vec::new();
    for root in roots {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            out.push(java_binary(&entry.path()));
        }
    }
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        out.push(java_binary(Path::new(&home)));
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        out.extend(std::env::split_paths(&path_var).map(|d| java_binary(&d)));
    }
    out.retain(|p| p.is_file());
    out
}

async fn probe(path: PathBuf) -> Option<JavaRuntime> {
    let mut cmd = Command::new(&path);
    cmd.arg("-version").kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(5), cmd.output())
        .await
        .ok()?
        .ok()?;
    let text = String::from_utf8_lossy(&output.stderr).into_owned()
        + &String::from_utf8_lossy(&output.stdout);
    let (major, version) = parse_java_version(&text)?;
    Some(JavaRuntime {
        path,
        major,
        version,
    })
}

/// Busca instalaciones de Java conocidas (sin ejecutar nada fuera de esas ubicaciones).
pub async fn detect(paths: &NebulaPaths) -> Vec<JavaRuntime> {
    let mut seen = HashSet::new();
    let mut tasks = tokio::task::JoinSet::new();
    for candidate in candidate_paths(paths) {
        let canonical = fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
        if seen.insert(canonical) {
            tasks.spawn(probe(candidate));
        }
    }
    let mut found = Vec::new();
    while let Some(res) = tasks.join_next().await {
        if let Ok(Some(rt)) = res {
            found.push(rt);
        }
    }
    found.sort_by(|a, b| a.major.cmp(&b.major).then_with(|| a.path.cmp(&b.path)));
    found
}

/// Solo se permite elegir un Java de la lista detectada (no rutas arbitrarias).
pub fn ensure_known(runtimes: &[JavaRuntime], choice: &JavaChoice) -> Result<()> {
    match choice {
        JavaChoice::Auto => Ok(()),
        JavaChoice::Path(p) => {
            if runtimes.iter().any(|r| &r.path == p) {
                Ok(())
            } else {
                Err(CoreError::invalid(
                    "ese Java no está en la lista de runtimes detectados",
                ))
            }
        }
    }
}

/// Versión mayor de Java que pide esta versión de Minecraft (8 si no lo indica).
pub fn required_major(meta: &VersionMeta) -> u32 {
    meta.java_version.as_ref().map_or(8, |j| j.major_version)
}

fn install_hint(major: u32) -> String {
    format!(
        "Nebula intentará descargar Java {major} automáticamente; si falla, revisa la conexión."
    )
}

/// Elige el runtime para lanzar `meta`.
///
/// * `Auto`: coincidencia exacta; si no, el menor superior, salvo que la versión
///   sea anterior a 1.13 y pida Java 8 (ahí un Java más nuevo suele fallar).
/// * `Path`: debe ser detectado y cumplir el mínimo (y, para esas versiones antiguas, ser Java 8).
pub fn resolve<'a>(
    runtimes: &'a [JavaRuntime],
    choice: &JavaChoice,
    meta: &VersionMeta,
) -> Result<&'a JavaRuntime> {
    let required = required_major(meta);
    let legacy = required == 8
        && meta
            .release_time
            .as_deref()
            .is_some_and(|t| t < JAVA8_ONLY_BEFORE);
    let acceptable = |r: &JavaRuntime| {
        if legacy {
            r.major == 8
        } else {
            r.major >= required
        }
    };

    match choice {
        JavaChoice::Path(p) => {
            let rt = runtimes.iter().find(|r| &r.path == p).ok_or_else(|| {
                CoreError::Launch(format!(
                    "el Java elegido ({}) ya no está disponible; elige otro o usa Automático",
                    p.display()
                ))
            })?;
            if acceptable(rt) {
                Ok(rt)
            } else {
                Err(CoreError::Launch(format!(
                    "Minecraft {} necesita Java {}{}; el elegido es Java {}",
                    meta.id,
                    required,
                    if legacy { " (exactamente)" } else { " o superior" },
                    rt.major
                )))
            }
        }
        JavaChoice::Auto => runtimes
            .iter()
            .filter(|r| acceptable(r))
            .min_by_key(|r| (r.major != required, r.major))
            .ok_or_else(|| {
                CoreError::Launch(format!(
                    "Minecraft {} necesita Java {}. {}",
                    meta.id,
                    required,
                    install_hint(required)
                ))
            }),
    }
}

/// Garantiza que hay un Java válido para `meta`. En modo automático, si no hay
/// instalado exactamente la versión mayor que pide Minecraft, descarga un JRE
/// de Adoptium en `data/java/` y vuelve a detectar. Con una ruta elegida a mano
/// no descarga nada: valida la elección del usuario.
pub async fn ensure_for_meta(
    paths: &NebulaPaths,
    choice: &JavaChoice,
    meta: &VersionMeta,
    reporter: &Reporter,
) -> Result<JavaRuntime> {
    let required = required_major(meta);
    let mut runtimes = detect(paths).await;

    if matches!(choice, JavaChoice::Auto) && !runtimes.iter().any(|r| r.major == required) {
        let installed = install_major(paths, required, reporter).await.map_err(|e| {
            CoreError::Launch(format!(
                "Minecraft {} necesita Java {} y no se pudo instalar automáticamente: {e}",
                meta.id, required
            ))
        })?;
        runtimes.retain(|r| r.path != installed.path);
        runtimes.push(installed);
        runtimes.sort_by(|a, b| a.major.cmp(&b.major).then_with(|| a.path.cmp(&b.path)));
    }

    resolve(&runtimes, choice, meta).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minecraft::meta::fixtures::{LEGACY, MODERN};

    fn rt(major: u32) -> JavaRuntime {
        JavaRuntime {
            path: PathBuf::from(format!("/jvm/{major}/bin/java")),
            major,
            version: major.to_string(),
        }
    }

    fn meta(json: &str) -> VersionMeta {
        serde_json::from_str(json).expect("fixture")
    }

    #[test]
    fn parses_real_java_version_outputs() {
        let cases = [
            ("openjdk version \"1.8.0_392\" 2023-10-17\nOpenJDK Runtime", 8),
            ("openjdk version \"17.0.9\" 2023-10-17", 17),
            ("openjdk version \"21\" 2023-09-19", 21),
            ("java version \"25-ea\" 2025-09-16", 25),
        ];
        for (text, major) in cases {
            assert_eq!(parse_java_version(text).map(|v| v.0), Some(major), "{text}");
        }
        assert!(parse_java_version("command not found").is_none());
    }

    #[test]
    fn auto_prefers_exact_then_lowest_higher() {
        let runtimes = [rt(8), rt(17), rt(21), rt(25)];
        let modern = meta(MODERN); // pide 21
        assert_eq!(resolve(&runtimes, &JavaChoice::Auto, &modern).map(|r| r.major).ok(), Some(21));
        let without21 = [rt(8), rt(17), rt(25)];
        assert_eq!(resolve(&without21, &JavaChoice::Auto, &modern).map(|r| r.major).ok(), Some(25));
        let too_old = [rt(8), rt(17)];
        let err = resolve(&too_old, &JavaChoice::Auto, &modern).err().map(|e| e.to_string());
        assert!(err.is_some_and(|m| m.contains("Java 21") && m.contains("automáticamente")));
    }

    #[test]
    fn legacy_versions_only_accept_java_8() {
        let legacy = meta(LEGACY); // 1.8.9 (2015), sin javaVersion → 8
        let only_new = [rt(17), rt(21)];
        assert!(resolve(&only_new, &JavaChoice::Auto, &legacy).is_err());
        let both = [rt(8), rt(17)];
        assert_eq!(resolve(&both, &JavaChoice::Auto, &legacy).map(|r| r.major).ok(), Some(8));
        // Elegir manualmente Java 17 para 1.8.9 se rechaza con un mensaje claro.
        let choice = JavaChoice::Path(PathBuf::from("/jvm/17/bin/java"));
        let err = resolve(&both, &choice, &legacy).err().map(|e| e.to_string());
        assert!(err.is_some_and(|m| m.contains("exactamente")));
    }

    #[test]
    fn manual_choice_must_be_detected() {
        let runtimes = [rt(21)];
        let modern = meta(MODERN);
        assert!(ensure_known(&runtimes, &JavaChoice::Path("/tmp/evil/java".into())).is_err());
        assert!(ensure_known(&runtimes, &JavaChoice::Path("/jvm/21/bin/java".into())).is_ok());
        assert!(resolve(&runtimes, &JavaChoice::Path("/tmp/evil/java".into()), &modern).is_err());
    }
}
