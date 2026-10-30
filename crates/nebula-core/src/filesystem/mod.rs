//! Operaciones de archivos pequeñas y seguras.

use std::{
    fs, io,
    io::Write,
    path::{Component, Path, PathBuf},
};

use sha1::{Digest, Sha1};

use crate::{error::CoreError, Result};

pub fn create_dir_all(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|e| CoreError::io(path, e))
}

/// Crea un archivo temporal nuevo (sin seguir symlinks) en el mismo directorio
/// que `path`, para que el `rename` posterior sea atómico.
fn create_temp_sibling(path: &Path) -> Result<(PathBuf, fs::File)> {
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::invalid("ruta sin directorio padre"))?;
    create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .ok_or_else(|| CoreError::invalid("ruta sin nombre de archivo"))?;
    for _ in 0..16 {
        let mut tmp_name = file_name.to_os_string();
        tmp_name.push(format!(".{}.tmp", rand::random::<u64>()));
        let tmp = parent.join(tmp_name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(file) => return Ok((tmp, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(CoreError::io(&tmp, e)),
        }
    }
    Err(CoreError::invalid(
        "no se pudo crear un archivo temporal único",
    ))
}

/// Escribe `bytes` de forma atómica: archivo temporal hermano + `rename`.
/// El temporal se crea con `create_new`, así que un symlink preexistente no
/// puede redirigir la escritura. Un corte de luz nunca deja un JSON a medias.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let (tmp, mut file) = create_temp_sibling(path)?;
    let write_result = file
        .write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| CoreError::io(&tmp, e));
    drop(file);
    if let Err(e) = write_result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }

    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            // En Windows `rename` no reemplaza un archivo existente. Se borra el
            // destino y se reintenta; si el segundo intento falla, no se deja el
            // temporal suelto.
            #[cfg(windows)]
            if path.is_file() && fs::remove_file(path).is_ok() {
                if fs::rename(&tmp, path).is_ok() {
                    return Ok(());
                }
            }
            let _ = fs::remove_file(&tmp);
            Err(CoreError::io(path, e))
        }
    }
}

fn hash_sidecar_path(path: &Path) -> PathBuf {
    let mut sidecar = path.as_os_str().to_os_string();
    sidecar.push(".sha1");
    PathBuf::from(sidecar)
}

pub fn sha1_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Deja al lado de la caché el SHA-1 del contenido que se acaba de escribir.
pub fn write_hash_sidecar(path: &Path, bytes: &[u8]) -> Result<()> {
    write_atomic(&hash_sidecar_path(path), sha1_hex(bytes).as_bytes())
}

/// Escribe el archivo y su hash lateral de forma atómica.
pub fn write_verified(path: &Path, bytes: &[u8]) -> Result<()> {
    write_atomic(path, bytes)?;
    write_hash_sidecar(path, bytes)
}

/// Lee una caché solo si su hash lateral coincide. Si falta el hash o no
/// coincide, devuelve `None` para forzar una descarga/verificación nueva.
pub fn read_verified(path: &Path) -> Option<Vec<u8>> {
    let bytes = fs::read(path).ok()?;
    let expected = fs::read_to_string(hash_sidecar_path(path)).ok()?;
    if sha1_hex(&bytes).eq_ignore_ascii_case(expected.trim()) {
        Some(bytes)
    } else {
        None
    }
}

/// Une `base` con una ruta *relativa* no confiable (p. ej. la de un archivo dentro
/// de un zip o de un mod). Rechaza rutas absolutas, `..`, prefijos, vacías y
/// symlinks ya existentes en cualquier componente. La comprobación del symlink
/// es defensa en profundidad: el `rename` atómico de `write_atomic` no sigue
/// enlaces, pero otras escrituras (zip, descargas) sí podrían hacerlo.
pub fn join_safe(base: &Path, untrusted: &str) -> Result<PathBuf> {
    let rel = Path::new(untrusted);
    let mut out = base.to_path_buf();
    let mut any = false;
    for comp in rel.components() {
        match comp {
            Component::Normal(part) => {
                out.push(part);
                if let Ok(metadata) = fs::symlink_metadata(&out) {
                    if metadata.file_type().is_symlink() {
                        return Err(CoreError::invalid(format!(
                            "ruta con symlink no permitida: {untrusted:?}"
                        )));
                    }
                }
                any = true;
            }
            Component::CurDir => {}
            _ => {
                return Err(CoreError::invalid(format!(
                    "ruta no permitida: {untrusted:?}"
                )))
            }
        }
    }
    if any {
        Ok(out)
    } else {
        Err(CoreError::invalid("ruta vacía"))
    }
}

/// Valida un componente de ruta que viene de datos externos (id de versión, de
/// asset index, de config de logging): solo `[A-Za-z0-9._-]`, sin `..`.
pub fn validate_component(s: &str) -> Result<&str> {
    let ok = !s.is_empty()
        && s.len() <= 64
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(s)
    } else {
        Err(CoreError::invalid(format!("identificador no válido: {s:?}")))
    }
}

/// Cuenta archivos directos de `dir` cuyo nombre termina en alguno de `suffixes`.
/// Un directorio inexistente cuenta como 0.
pub fn count_files_with_suffix(dir: &Path, suffixes: &[&str]) -> Result<usize> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(CoreError::io(dir, e)),
    };
    let mut n = 0;
    for entry in entries {
        let entry = entry.map_err(|e| CoreError::io(dir, e))?;
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if entry.path().is_file() && suffixes.iter().any(|s| name.ends_with(s)) {
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_safe_accepts_nested_relative() {
        let p = join_safe(Path::new("/base"), "mods/a.jar").unwrap_or_default();
        assert_eq!(p, Path::new("/base/mods/a.jar"));
    }

    #[test]
    fn join_safe_rejects_traversal_and_absolute() {
        for bad in ["../x", "a/../../x", "/etc/passwd", "", "."] {
            assert!(join_safe(Path::new("/base"), bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn write_verified_detects_corruption() {
        let dir = std::env::temp_dir().join(format!(
            "nebula-fs-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        create_dir_all(&dir).expect("dir");
        let path = dir.join("cache.json");
        write_verified(&path, b"contenido").expect("write");
        assert_eq!(read_verified(&path).as_deref(), Some(&b"contenido"[..]));
        fs::write(&path, b"manipulado").expect("corrupt");
        assert!(read_verified(&path).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn join_safe_rejects_existing_symlink_components() {
        use std::os::unix::fs::symlink;

        let dir = std::env::temp_dir().join(format!(
            "nebula-fs-link-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        create_dir_all(&dir).expect("dir");
        let outside = dir.join("outside");
        create_dir_all(&outside).expect("outside");
        symlink(&outside, dir.join("link")).expect("symlink");
        assert!(join_safe(&dir, "link/evil.txt").is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
