//! Imagen de fondo personalizada por instancia (PNG, JPEG o WEBP), guardada
//! como `background.<ext>` dentro de la propia carpeta de la instancia. Sin
//! imagen propia, el frontend dibuja un fondo predeterminado solo con CSS
//! (gradientes), así que aquí no hace falta ningún archivo "por defecto".

use std::{fs, path::Path};

use crate::{error::CoreError, filesystem, Result};

/// 15 MB es de sobra para una imagen de fondo y evita que un IPC
/// descontrolado (o un archivo elegido por error) agote memoria.
const MAX_BACKGROUND_BYTES: usize = 15 * 1024 * 1024;

const KNOWN: &[(&str, &str)] = &[("image/png", "png"), ("image/jpeg", "jpg"), ("image/webp", "webp")];

fn ext_for_mime(mime: &str) -> Result<&'static str> {
    KNOWN
        .iter()
        .find(|(m, _)| *m == mime)
        .map(|(_, ext)| *ext)
        .ok_or_else(|| CoreError::invalid("formato de imagen no admitido (usa PNG, JPG o WEBP)"))
}

/// Comprobación ligera de que los bytes empiezan de verdad como ese formato
/// (no es una validación completa del archivo, solo descarta el caso típico
/// de una extensión/mime que no corresponde con el contenido real).
fn looks_like(mime: &str, bytes: &[u8]) -> bool {
    match mime {
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => bytes.starts_with(b"\xFF\xD8\xFF"),
        "image/webp" => bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
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

fn candidate_path(instance_dir: &Path, ext: &str) -> std::path::PathBuf {
    instance_dir.join(format!("background.{ext}"))
}

/// Guarda `bytes` como fondo de la instancia, sustituyendo cualquier fondo
/// anterior (aunque tuviera otra extensión).
pub fn save(instance_dir: &Path, mime: &str, bytes: &[u8]) -> Result<()> {
    let ext = ext_for_mime(mime)?;
    if bytes.len() > MAX_BACKGROUND_BYTES {
        return Err(CoreError::invalid("la imagen supera el tamaño máximo admitido (15 MB)"));
    }
    if !looks_like(mime, bytes) {
        return Err(CoreError::invalid(
            "el archivo no parece ser una imagen válida de ese formato",
        ));
    }
    clear(instance_dir)?;
    filesystem::write_atomic(&candidate_path(instance_dir, ext), bytes)
}

/// Guarda bytes cuyo formato se detecta por la firma (PNG/JPEG/WEBP).
pub fn save_detected(instance_dir: &Path, bytes: &[u8]) -> Result<()> {
    let mime = sniff_mime(bytes)
        .ok_or_else(|| CoreError::invalid("el fondo remoto no es PNG, JPG ni WEBP"))?;
    save(instance_dir, mime, bytes)
}

/// `(mime, bytes)` del fondo actual, o `None` si la instancia no tiene uno.
pub fn load(instance_dir: &Path) -> Result<Option<(String, Vec<u8>)>> {
    for (mime, ext) in KNOWN {
        let path = candidate_path(instance_dir, ext);
        if path.is_file() {
            let bytes = fs::read(&path).map_err(|e| CoreError::io(&path, e))?;
            return Ok(Some(((*mime).to_owned(), bytes)));
        }
    }
    Ok(None)
}

/// Borra el fondo de la instancia si tiene uno. No es un error que no exista.
pub fn clear(instance_dir: &Path) -> Result<()> {
    for (_, ext) in KNOWN {
        let path = candidate_path(instance_dir, ext);
        if path.is_file() {
            fs::remove_file(&path).map_err(|e| CoreError::io(&path, e))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::test_util::TempDir;

    const PNG_1X1: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR";
    const JPEG_HEAD: &[u8] = b"\xFF\xD8\xFF\xE0\x00\x10JFIF";

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
    fn rejects_unsupported_mime_mismatched_content_and_oversize() {
        let tmp = TempDir::new();
        assert!(save(tmp.path(), "image/gif", b"GIF89a").is_err());
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
        assert_eq!(sniff_mime(b"GIF89a"), None);
        let tmp = TempDir::new();
        save_detected(tmp.path(), JPEG_HEAD).expect("save detected");
        assert_eq!(load(tmp.path()).expect("load").map(|(m, _)| m), Some("image/jpeg".into()));
    }
}
