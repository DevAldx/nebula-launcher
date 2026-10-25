//! Operaciones de archivos pequeñas y seguras.

use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use crate::{error::CoreError, Result};

pub fn create_dir_all(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|e| CoreError::io(path, e))
}

/// Escribe `bytes` de forma atómica: archivo temporal hermano + `rename`.
/// Un corte de luz nunca deja un JSON a medias.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::invalid("ruta sin directorio padre"))?;
    create_dir_all(parent)?;
    let mut tmp_name = path
        .file_name()
        .ok_or_else(|| CoreError::invalid("ruta sin nombre de archivo"))?
        .to_os_string();
    tmp_name.push(".tmp");
    let tmp = parent.join(tmp_name);
    fs::write(&tmp, bytes).map_err(|e| CoreError::io(&tmp, e))?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            // En Windows `rename` no reemplaza un archivo existente. Se borra el
            // destino y se reintenta; si el segundo intento falla, no se deja el
            // `.tmp` suelto.
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

/// Une `base` con una ruta *relativa* no confiable (p. ej. la de un archivo dentro
/// de un zip o de un mod). Rechaza rutas absolutas, `..`, prefijos y vacías.
/// La comprobación es léxica: no sigue symlinks ni exige que exista.
pub fn join_safe(base: &Path, untrusted: &str) -> Result<PathBuf> {
    let rel = Path::new(untrusted);
    let mut out = base.to_path_buf();
    let mut any = false;
    for comp in rel.components() {
        match comp {
            Component::Normal(part) => {
                out.push(part);
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
}
