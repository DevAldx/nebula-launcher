//! Descarga e instalación de un JRE de Eclipse Temurin (Adoptium) cuando la
//! instancia necesita una versión de Java que no está en el sistema.
//!
//! El paquete se descarga por HTTPS desde la API oficial de Adoptium, se
//! verifica con el SHA-256 que publica esa misma API y se extrae en
//! `data/java/temurin-<mayor>`. Nunca se ejecuta nada del archivo hasta que la
//! verificación ha terminado.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use flate2::read::GzDecoder;
use serde::Deserialize;
use tar::Archive;

use super::{java_binary, probe, JavaRuntime};
use crate::{
    downloads::{Downloader, Task},
    error::CoreError,
    filesystem::{self, validate_component},
    minecraft::Reporter,
    NebulaPaths, Result,
};

/// Hosts que pueden intervenir en la descarga (la API redirige a GitHub).
pub const JAVA_HOSTS: &[&str] = &[
    "api.adoptium.net",
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "github-releases.githubusercontent.com",
];

const MAX_API_BYTES: usize = 1024 * 1024;

#[derive(Debug, Deserialize)]
struct Asset {
    binary: Binary,
}

#[derive(Debug, Deserialize)]
struct Binary {
    package: Package,
}

#[derive(Debug, Deserialize)]
struct Package {
    link: String,
    checksum: String,
    name: String,
    size: Option<u64>,
}

fn adoptium_os() -> Result<&'static str> {
    if cfg!(target_os = "windows") {
        Ok("windows")
    } else if cfg!(target_os = "linux") {
        Ok("linux")
    } else if cfg!(target_os = "macos") {
        Ok("mac")
    } else {
        Err(CoreError::invalid(
            "la instalación automática de Java no está disponible en este sistema",
        ))
    }
}

fn adoptium_arch() -> Result<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("x64"),
        "aarch64" => Ok("aarch64"),
        other => Err(CoreError::invalid(format!(
            "arquitectura no soportada para Java automático: {other}"
        ))),
    }
}

fn api_url(major: u32) -> Result<String> {
    Ok(format!(
        "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture={}&image_type=jre&os={}&vendor=eclipse",
        adoptium_arch()?,
        adoptium_os()?
    ))
}

/// Ruta estable del JRE gestionado por Nebula para esa versión mayor.
pub fn install_dir(paths: &NebulaPaths, major: u32) -> PathBuf {
    paths.java_dir().join(format!("temurin-{major}"))
}

fn normalize_checksum(raw: &str) -> &str {
    raw.trim()
        .strip_prefix("sha256:")
        .unwrap_or_else(|| raw.trim())
}

/// Descarga e instala el JRE exacto que pide `major`. Si ya existe uno válido,
/// no toca la red.
pub async fn install_major(
    paths: &NebulaPaths,
    major: u32,
    reporter: &Reporter,
) -> Result<JavaRuntime> {
    let target = install_dir(paths, major);
    let existing = java_binary(&target);
    if existing.is_file() {
        if let Some(rt) = probe(existing).await {
            if rt.major == major {
                return Ok(rt);
            }
        }
        // Restos de una instalación anterior que no arranca: se reemplazan.
        let _ = fs::remove_dir_all(&target);
    }

    let dl = Downloader::new(JAVA_HOSTS)?;
    reporter.phase(&format!("Buscando Java {major}"), 0);
    let bytes = dl.fetch_bytes(&api_url(major)?, MAX_API_BYTES).await?;
    let assets: Vec<Asset> = serde_json::from_slice(&bytes)
        .map_err(|e| CoreError::Network(format!("respuesta de Adoptium inválida: {e}")))?;
    let package = assets
        .into_iter()
        .next()
        .map(|a| a.binary.package)
        .ok_or_else(|| CoreError::Network(format!("Adoptium no ofrece un JRE {major} para este sistema")))?;

    let file_name = validate_component(&package.name)?;
    let downloads = paths.java_dir().join("downloads");
    filesystem::create_dir_all(&downloads)?;
    let archive = downloads.join(file_name);

    reporter.phase(&format!("Descargando Java {major}"), package.size.unwrap_or(0));
    let task = Task {
        url: package.link,
        dest: archive.clone(),
        sha1: None,
        sha256: Some(normalize_checksum(&package.checksum).to_owned()),
        size: package.size,
    };
    dl.download(&task, &|delta| reporter.add(delta)).await?;

    reporter.phase(&format!("Instalando Java {major}"), 0);
    let archive2 = archive.clone();
    let target2 = target.clone();
    let major2 = major;
    tokio::task::spawn_blocking(move || extract_and_install(&archive2, &target2, major2))
        .await
        .map_err(|e| CoreError::Launch(format!("no se pudo extraer Java: {e}")))??;

    let rt = probe(java_binary(&target))
        .await
        .ok_or_else(|| CoreError::Launch(format!("Java {major} se instaló pero no arranca")))?;
    if rt.major != major {
        return Err(CoreError::Launch(format!(
            "se esperaba Java {major} y se instaló Java {}",
            rt.major
        )));
    }
    let _ = fs::remove_file(&archive);
    Ok(rt)
}

fn extraction_staging(paths_java_dir: &Path, major: u32) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    paths_java_dir.join(format!(".staging-{major}-{stamp}"))
}

fn extract_and_install(archive: &Path, target: &Path, major: u32) -> Result<()> {
    let java_dir = target
        .parent()
        .ok_or_else(|| CoreError::invalid("ruta de Java inválida"))?;
    let staging = extraction_staging(java_dir, major);
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|e| CoreError::io(&staging, e))?;
    }
    filesystem::create_dir_all(&staging)?;

    let name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let extracted = if name.ends_with(".zip") {
        crate::minecraft::install::extract_zip(archive, &staging, &[])
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_tar_gz(archive, &staging)
    } else {
        Err(CoreError::invalid(format!(
            "formato de Java no soportado: {}",
            archive.display()
        )))
    };
    if let Err(e) = extracted {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }

    let root = find_root(&staging);
    if !java_binary(&root).is_file() {
        let _ = fs::remove_dir_all(&staging);
        return Err(CoreError::Integrity(
            "el paquete de Java no contiene bin/java".into(),
        ));
    }

    if target.exists() {
        fs::remove_dir_all(target).map_err(|e| CoreError::io(target, e))?;
    }
    match fs::rename(&root, target) {
        Ok(()) => {}
        Err(e) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(CoreError::io(target, e));
        }
    }
    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    Ok(())
}

/// El tar/zip de Adoptium trae una única carpeta raíz (`jdk-…`). Si no, se usa
/// el propio directorio de extracción.
fn find_root(staging: &Path) -> PathBuf {
    let mut dirs = Vec::new();
    let mut files = 0usize;
    if let Ok(entries) = fs::read_dir(staging) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                dirs.push(entry.path());
            } else {
                files += 1;
            }
        }
    }
    if files == 0 && dirs.len() == 1 {
        dirs.remove(0)
    } else {
        staging.to_path_buf()
    }
}

fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<()> {
    let file = fs::File::open(archive).map_err(|e| CoreError::io(archive, e))?;
    let mut tar = Archive::new(GzDecoder::new(file));
    let entries = tar
        .entries()
        .map_err(|e| CoreError::Integrity(format!("{}: {e}", archive.display())))?;
    for entry in entries {
        let mut entry =
            entry.map_err(|e| CoreError::Integrity(format!("{}: {e}", archive.display())))?;
        let relative = entry
            .path()
            .map_err(|e| CoreError::Integrity(format!("{}: {e}", archive.display())))?
            .to_string_lossy()
            .into_owned();
        let target = filesystem::join_safe(dest, &relative)?;
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            filesystem::create_dir_all(&target)?;
        } else if kind.is_file() {
            if let Some(parent) = target.parent() {
                filesystem::create_dir_all(parent)?;
            }
            entry
                .unpack(&target)
                .map_err(|e| CoreError::io(&target, e))?;
        }
        // Los enlaces simbólicos se omiten: Java no los necesita para arrancar
        // y así una entrada maliciosa no puede apuntar fuera de la carpeta.
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::test_util::TempDir;

    #[test]
    fn checksum_prefix_is_optional() {
        assert_eq!(normalize_checksum("abcdef"), "abcdef");
        assert_eq!(normalize_checksum(" sha256:abcdef "), "abcdef");
    }

    #[test]
    fn finds_the_single_root_folder() {
        let tmp = TempDir::new();
        let root = tmp.path().join("jdk-21.0.1+12-jre");
        fs::create_dir_all(root.join("bin")).unwrap_or_default();
        fs::write(root.join("bin/java"), b"x").unwrap_or_default();
        assert_eq!(find_root(tmp.path()), root);
    }

    #[test]
    fn install_dir_is_stable() {
        let paths = NebulaPaths::new("/c", "/d");
        assert_eq!(install_dir(&paths, 21), PathBuf::from("/d/java/temurin-21"));
    }
}
