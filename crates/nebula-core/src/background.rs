//! Fondos e iconos personalizados.
//!
//! Los fondos de instancia se guardan como `background.<ext>` dentro de la
//! carpeta de la instancia. El fondo y el icono del launcher viven en el
//! directorio de configuración (`launcher-background.<ext>` y
//! `launcher-icon.png`) para que no dependan de una instancia concreta.
//!
//! Se admiten PNG, JPEG, WEBP y GIF. Los GIF y los WEBP animados se reproducen
//! tal cual en el fondo del launcher; no se convierten ni se recodifican.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{error::CoreError, filesystem, Result};

/// 15 MB es de sobra para un fondo y evita que un IPC descontrolado (o un
/// archivo elegido por error) agote memoria.
const MAX_BACKGROUND_BYTES: usize = 15 * 1024 * 1024;

/// Un icono se muestra en la barra lateral y como icono de ventana. No
/// necesita más resolución que un PNG cuadrado.
pub const MAX_ICON_BYTES: usize = 4 * 1024 * 1024;

pub const LAUNCHER_BACKGROUND_STEM: &str = "launcher-background";
pub const LAUNCHER_ICON_STEM: &str = "launcher-icon";

const KNOWN: &[(&str, &str)] = &[
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/webp", "webp"),
    ("image/gif", "gif"),
];

fn ext_for_mime(mime: &str) -> Result<&'static str> {
    KNOWN
        .iter()
        .find(|(m, _)| *m == mime)
        .map(|(_, ext)| *ext)
        .ok_or_else(|| CoreError::invalid("formato de imagen no admitido (usa PNG, JPG, WEBP o GIF)"))
}

/// Comprobación ligera de que los bytes empiezan de verdad como ese formato
/// (no es una validación completa del archivo, solo descarta el caso típico
/// de una extensión/mime que no corresponde con el contenido real).
fn looks_like(mime: &str, bytes: &[u8]) -> bool {
    match mime {
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => bytes.starts_with(b"\xFF\xD8\xFF"),
        "image/webp" => bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
        "image/gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        _ => false,
    }
}

/// Detecta el formato por la firma del archivo. Pensado para fondos que llegan
/// desde un manifiesto remoto, donde no hay un `Content-Type` fiable.
pub fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
    KNOWN
        .iter()
        .map(|(mime, _)| *mime)
        .find(|mime| looks_like(mime, bytes))
}

/// Lee ancho y alto de la cabecera IHDR de un PNG sin decodificar los píxeles.
/// Sirve para rechazar iconos enormes antes de que el decodificador reserve
/// memoria por ellos.
pub fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24
        || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || &bytes[12..16] != b"IHDR"
    {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((width, height))
}

fn validate_stem(stem: &str) -> Result<()> {
    if stem.is_empty()
        || !stem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(CoreError::invalid("nombre de archivo de imagen no válido"));
    }
    Ok(())
}

fn candidate_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    dir.join(format!("{stem}.{ext}"))
}

/// Guarda una imagen con un nombre base concreto (`background`,
/// `launcher-background`, `launcher-icon`…) sustituyendo cualquier versión
/// anterior, aunque tuviera otra extensión.
pub fn save_named(dir: &Path, stem: &str, mime: &str, bytes: &[u8]) -> Result<()> {
    validate_stem(stem)?;
    let ext = ext_for_mime(mime)?;
    if bytes.len() > MAX_BACKGROUND_BYTES {
        return Err(CoreError::invalid("la imagen supera el tamaño máximo admitido (15 MB)"));
    }
    if !looks_like(mime, bytes) {
        return Err(CoreError::invalid(
            "el archivo no parece ser una imagen válida de ese formato",
        ));
    }
    clear_named(dir, stem)?;
    filesystem::write_atomic(&candidate_path(dir, stem, ext), bytes)
}

/// `(mime, bytes)` de una imagen guardada con ese nombre base, o `None`.
pub fn load_named(dir: &Path, stem: &str) -> Result<Option<(String, Vec<u8>)>> {
    validate_stem(stem)?;
    for (mime, ext) in KNOWN {
        let path = candidate_path(dir, stem, ext);
        if path.is_file() {
            let bytes = fs::read(&path).map_err(|e| CoreError::io(&path, e))?;
            return Ok(Some(((*mime).to_owned(), bytes)));
        }
    }
    Ok(None)
}

/// Borra una imagen guardada con ese nombre base. No es un error que no exista.
pub fn clear_named(dir: &Path, stem: &str) -> Result<()> {
    validate_stem(stem)?;
    for (_, ext) in KNOWN {
        let path = candidate_path(dir, stem, ext);
        if path.is_file() {
            fs::remove_file(&path).map_err(|e| CoreError::io(&path, e))?;
        }
    }
    Ok(())
}

// ───────────────────────────── Fondo de instancia ─────────────────────────────

/// Guarda `bytes` como fondo de la instancia, sustituyendo cualquier fondo
/// anterior (aunque tuviera otra extensión).
pub fn save(instance_dir: &Path, mime: &str, bytes: &[u8]) -> Result<()> {
    save_named(instance_dir, "background", mime, bytes)
}

/// Guarda bytes cuyo formato se detecta por la firma (PNG/JPEG/WEBP/GIF).
pub fn save_detected(instance_dir: &Path, bytes: &[u8]) -> Result<()> {
    let mime = sniff_mime(bytes)
        .ok_or_else(|| CoreError::invalid("el fondo remoto no es PNG, JPG, WEBP ni GIF"))?;
    save(instance_dir, mime, bytes)
}

/// `(mime, bytes)` del fondo actual, o `None` si la instancia no tiene uno.
pub fn load(instance_dir: &Path) -> Result<Option<(String, Vec<u8>)>> {
    load_named(instance_dir, "background")
}

/// Borra el fondo de la instancia si tiene uno. No es un error que no exista.
pub fn clear(instance_dir: &Path) -> Result<()> {
    clear_named(instance_dir, "background")
}

// ───────────────────────────── Launcher ─────────────────────────────

pub fn save_launcher_background(config_dir: &Path, mime: &str, bytes: &[u8]) -> Result<()> {
    save_named(config_dir, LAUNCHER_BACKGROUND_STEM, mime, bytes)
}

pub fn load_launcher_background(config_dir: &Path) -> Result<Option<(String, Vec<u8>)>> {
    load_named(config_dir, LAUNCHER_BACKGROUND_STEM)
}

pub fn clear_launcher_background(config_dir: &Path) -> Result<()> {
    clear_named(config_dir, LAUNCHER_BACKGROUND_STEM)
}

/// El icono se limita a PNG porque Tauri lo usa también como icono de ventana
/// (`Image::from_bytes` solo decodifica PNG/ICO con las features actuales).
pub fn save_launcher_icon(config_dir: &Path, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_ICON_BYTES {
        return Err(CoreError::invalid(
            "el icono supera el tamaño máximo admitido (4 MB)",
        ));
    }
    save_named(config_dir, LAUNCHER_ICON_STEM, "image/png", bytes)
}

pub fn load_launcher_icon(config_dir: &Path) -> Result<Option<(String, Vec<u8>)>> {
    load_named(config_dir, LAUNCHER_ICON_STEM)
}

pub fn clear_launcher_icon(config_dir: &Path) -> Result<()> {
    clear_named(config_dir, LAUNCHER_ICON_STEM)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::test_util::TempDir;

    const PNG_1X1: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR";
    const JPEG_HEAD: &[u8] = b"\xFF\xD8\xFF\xE0\x00\x10JFIF";
    const GIF_HEAD: &[u8] = b"GIF89a\x01\x00\x01\x00\x80\x00\x00";

    #[test]
    fn save_then_load_round_trips() {
        let tmp = TempDir::new();
        assert_eq!(load(tmp.path()).expect("load"), None);
        save(tmp.path(), "image/png", PNG_1X1).expect("save");
        let (mime, bytes) = load(tmp.path()).expect("load").expect("some");
        assert_eq!(mime, "image/png");
        assert_eq!(bytes, PNG_1X1);
    }

    #[test]
    fn saving_a_new_format_replaces_the_old_file() {
        let tmp = TempDir::new();
        save(tmp.path(), "image/png", PNG_1X1).expect("save png");
        save(tmp.path(), "image/jpeg", JPEG_HEAD).expect("save jpeg");
        assert!(!tmp.path().join("background.png").exists());
        let (mime, _) = load(tmp.path()).expect("load").expect("some");
        assert_eq!(mime, "image/jpeg");
    }

    #[test]
    fn accepts_gif_and_animated_webp_headers() {
        let tmp = TempDir::new();
        save(tmp.path(), "image/gif", GIF_HEAD).expect("save gif");
        assert_eq!(load(tmp.path()).expect("load").map(|(m, _)| m), Some("image/gif".into()));
        let webp = b"RIFF\x24\x00\x00\x00WEBPVP8 ";
        save(tmp.path(), "image/webp", webp).expect("save webp");
        assert_eq!(load(tmp.path()).expect("load").map(|(m, _)| m), Some("image/webp".into()));
    }

    #[test]
    fn rejects_unsupported_mime_mismatched_content_and_oversize() {
        let tmp = TempDir::new();
        assert!(save(tmp.path(), "image/bmp", b"BM").is_err());
        assert!(save(tmp.path(), "image/png", b"no soy un png").is_err());
        let huge = vec![0x89u8; MAX_BACKGROUND_BYTES + 1];
        assert!(save(tmp.path(), "image/png", &huge).is_err());
        assert_eq!(load(tmp.path()).expect("load"), None); // ningún intento fallido dejó archivo
    }

    #[test]
    fn clear_removes_file_and_is_a_noop_without_one() {
        let tmp = TempDir::new();
        assert!(clear(tmp.path()).is_ok());
        save(tmp.path(), "image/png", PNG_1X1).expect("save");
        clear(tmp.path()).expect("clear");
        assert_eq!(load(tmp.path()).expect("load"), None);
    }

    #[test]
    fn sniffs_the_format_from_content() {
        assert_eq!(sniff_mime(PNG_1X1), Some("image/png"));
        assert_eq!(sniff_mime(JPEG_HEAD), Some("image/jpeg"));
        assert_eq!(sniff_mime(GIF_HEAD), Some("image/gif"));
        let tmp = TempDir::new();
        save_detected(tmp.path(), JPEG_HEAD).expect("save detected");
        assert_eq!(load(tmp.path()).expect("load").map(|(m, _)| m), Some("image/jpeg".into()));
    }

    #[test]
    fn named_images_are_independent() {
        let tmp = TempDir::new();
        save_named(tmp.path(), "launcher-background", "image/png", PNG_1X1).expect("save bg");
        save_named(tmp.path(), "background", "image/jpeg", JPEG_HEAD).expect("save instance bg");
        assert_eq!(
            load_named(tmp.path(), "launcher-background")
                .expect("load")
                .map(|(m, _)| m),
            Some("image/png".into())
        );
        assert_eq!(
            load_named(tmp.path(), "background")
                .expect("load")
                .map(|(m, _)| m),
            Some("image/jpeg".into())
        );
    }

    #[test]
    fn rejects_unsafe_stems() {
        let tmp = TempDir::new();
        assert!(save_named(tmp.path(), "../fuera", "image/png", PNG_1X1).is_err());
        assert!(save_named(tmp.path(), "a/b", "image/png", PNG_1X1).is_err());
        assert!(load_named(tmp.path(), "").is_err());
    }

    #[test]
    fn reads_png_dimensions_from_the_header() {
        let mut png = Vec::from(b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".as_slice());
        png.extend_from_slice(&512u32.to_be_bytes());
        png.extend_from_slice(&512u32.to_be_bytes());
        assert_eq!(png_dimensions(&png), Some((512, 512)));
        assert_eq!(png_dimensions(b"no soy un png"), None);
        assert_eq!(png_dimensions(PNG_1X1), None);
    }

    #[test]
    fn launcher_icon_is_png_and_has_its_own_limit() {
        let tmp = TempDir::new();
        save_launcher_icon(tmp.path(), PNG_1X1).expect("save icon");
        assert_eq!(
            load_launcher_icon(tmp.path()).expect("load").map(|(m, _)| m),
            Some("image/png".into())
        );
        assert!(save_launcher_icon(tmp.path(), JPEG_HEAD).is_err());
        let huge = vec![0x89u8; MAX_ICON_BYTES + 1];
        assert!(save_launcher_icon(tmp.path(), &huge).is_err());
        clear_launcher_icon(tmp.path()).expect("clear");
        assert_eq!(load_launcher_icon(tmp.path()).expect("load"), None);
    }
}
