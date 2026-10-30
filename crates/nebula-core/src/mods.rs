//! Gestión de mods locales de una instancia: listar, activar/desactivar,
//! borrar e importar. Nada de esto toca la red; Modrinth u otras fuentes
//! remotas quedan para más adelante, como dice la especificación.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{error::CoreError, filesystem, Result};

const DISABLED_SUFFIX: &str = ".disabled";
/// Tope generoso para un solo archivo importado (algunos modpacks grandes
/// rondan decenas de MB); evita que un IPC descontrolado agote memoria.
const MAX_IMPORT_BYTES: usize = 200 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct ModEntry {
    /// Nombre real en disco (incluye `.disabled` cuando está desactivado).
    pub file_name: String,
    pub enabled: bool,
    pub size_bytes: u64,
}

fn validate_mod_filename(name: &str) -> Result<()> {
    let bad = name.is_empty()
        || name.len() > 255
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', '\0']);
    if bad {
        return Err(CoreError::invalid("nombre de mod no válido"));
    }
    let lower = name.to_ascii_lowercase();
    if !(lower.ends_with(".jar") || lower.ends_with(".jar.disabled")) {
        return Err(CoreError::invalid("solo se admiten archivos .jar"));
    }
    Ok(())
}

/// Mods de una instancia, ordenados por nombre. Una carpeta `mods/` inexistente
/// (instancia recién creada) cuenta como lista vacía, no como error.
pub fn list(mods_dir: &Path) -> Result<Vec<ModEntry>> {
    let entries = match fs::read_dir(mods_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(CoreError::io(mods_dir, e)),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| CoreError::io(mods_dir, e))?;
        if !entry.path().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();
        let enabled = lower.ends_with(".jar");
        let disabled = lower.ends_with(".jar.disabled");
        if !enabled && !disabled {
            continue; // archivos ajenos (notas, configs sueltas) no se listan como mods
        }
        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        out.push(ModEntry {
            file_name: name,
            enabled,
            size_bytes,
        });
    }
    out.sort_by(|a, b| a.file_name.to_ascii_lowercase().cmp(&b.file_name.to_ascii_lowercase()));
    Ok(out)
}

/// Activa o desactiva un mod renombrando `.jar` ↔ `.jar.disabled`.
/// Devuelve el nombre de archivo resultante.
pub fn set_enabled(mods_dir: &Path, file_name: &str, enabled: bool) -> Result<String> {
    validate_mod_filename(file_name)?;
    let from = mods_dir.join(file_name);
    if !from.is_file() {
        return Err(CoreError::NotFound(file_name.to_owned()));
    }
    let base = file_name.strip_suffix(DISABLED_SUFFIX).unwrap_or(file_name);
    let target_name = if enabled {
        base.to_owned()
    } else {
        format!("{base}{DISABLED_SUFFIX}")
    };
    if target_name == file_name {
        return Ok(file_name.to_owned()); // ya estaba en el estado pedido
    }
    let to = mods_dir.join(&target_name);
    if to.exists() {
        return Err(CoreError::invalid(format!(
            "ya existe un mod llamado {target_name}"
        )));
    }
    fs::rename(&from, &to).map_err(|e| CoreError::io(&to, e))?;
    Ok(target_name)
}

pub fn delete(mods_dir: &Path, file_name: &str) -> Result<()> {
    validate_mod_filename(file_name)?;
    let path = mods_dir.join(file_name);
    fs::remove_file(&path).map_err(|e| CoreError::io(&path, e))
}

/// Un archivo a importar: nombre original (solo se usa su componente final,
/// nunca una ruta) y contenido ya en memoria.
pub struct ImportFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Copia archivos `.jar` a `mods/`. Si ya existe un archivo con ese nombre,
/// añade un sufijo numérico en vez de sobrescribirlo. Devuelve los nombres
/// finales, en el mismo orden que `files`.
pub fn import(mods_dir: &Path, files: Vec<ImportFile>) -> Result<Vec<String>> {
    filesystem::create_dir_all(mods_dir)?;
    let mut written = Vec::with_capacity(files.len());
    for file in files {
        written.push(import_one(mods_dir, file)?);
    }
    Ok(written)
}

/// Importa un único mod. Permite al IPC decodificar y escribir de uno en uno
/// sin mantener todos los archivos en memoria a la vez.
pub fn import_one(mods_dir: &Path, file: ImportFile) -> Result<String> {
    filesystem::create_dir_all(mods_dir)?;
    let safe_name = sanitize_import_name(&file.name)?;
    if file.bytes.len() > MAX_IMPORT_BYTES {
        return Err(CoreError::invalid(format!(
            "{safe_name}: supera el tamaño máximo admitido"
        )));
    }
    let dest = unique_path(mods_dir, &safe_name);
    filesystem::write_atomic(&dest, &file.bytes)?;
    Ok(dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or(safe_name))
}

fn sanitize_import_name(original: &str) -> Result<String> {
    // Solo interesa el nombre; una ruta completa (aunque no debería llegar
    // desde la UI) se trata como no confiable y se reduce a su componente final.
    let name = Path::new(original)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name.is_empty() || name.len() > 255 || !name.to_ascii_lowercase().ends_with(".jar") {
        return Err(CoreError::invalid("solo se pueden importar archivos .jar"));
    }
    Ok(name)
}

fn unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = file_name.strip_suffix(".jar").unwrap_or(file_name);
    for n in 1..1000 {
        let alt = dir.join(format!("{stem} ({n}).jar"));
        if !alt.exists() {
            return alt;
        }
    }
    dir.join(format!("{stem}-{}.jar", rand::random::<u32>()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::test_util::TempDir;

    #[test]
    fn list_ignores_non_jars_and_reports_enabled_state() {
        let tmp = TempDir::new();
        let dir = tmp.path().join("mods");
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join("Sodium.jar"), b"aa").expect("w");
        fs::write(dir.join("Lithium.jar.disabled"), b"bbb").expect("w");
        fs::write(dir.join("readme.txt"), b"x").expect("w");

        let entries = list(&dir).expect("list");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].file_name, "Lithium.jar.disabled");
        assert!(!entries[0].enabled);
        assert_eq!(entries[0].size_bytes, 3);
        assert_eq!(entries[1].file_name, "Sodium.jar");
        assert!(entries[1].enabled);
    }

    #[test]
    fn list_on_missing_dir_is_empty_not_error() {
        let tmp = TempDir::new();
        assert_eq!(list(&tmp.path().join("nope")).expect("list").len(), 0);
    }

    #[test]
    fn set_enabled_toggles_both_ways_and_is_idempotent() {
        let tmp = TempDir::new();
        let dir = tmp.path().join("mods");
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join("a.jar"), b"x").expect("w");

        let name = set_enabled(&dir, "a.jar", false).expect("disable");
        assert_eq!(name, "a.jar.disabled");
        assert!(!dir.join("a.jar").exists());
        assert!(dir.join("a.jar.disabled").is_file());

        // Ya está desactivado: no-op, no error.
        assert_eq!(set_enabled(&dir, "a.jar.disabled", false).expect("noop"), "a.jar.disabled");

        let name = set_enabled(&dir, "a.jar.disabled", true).expect("enable");
        assert_eq!(name, "a.jar");
        assert!(dir.join("a.jar").is_file());
    }

    #[test]
    fn set_enabled_rejects_conflict_and_missing_and_bad_names() {
        let tmp = TempDir::new();
        let dir = tmp.path().join("mods");
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join("a.jar"), b"x").expect("w");
        fs::write(dir.join("a.jar.disabled"), b"y").expect("w");

        assert!(set_enabled(&dir, "a.jar", false).is_err()); // ya existe a.jar.disabled
        assert!(set_enabled(&dir, "missing.jar", true).is_err());
        for bad in ["../x.jar", "a/b.jar", "readme.txt", ""] {
            assert!(set_enabled(&dir, bad, true).is_err(), "{bad}");
        }
    }

    #[test]
    fn delete_removes_file() {
        let tmp = TempDir::new();
        let dir = tmp.path().join("mods");
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join("a.jar"), b"x").expect("w");
        assert!(delete(&dir, "a.jar").is_ok());
        assert!(!dir.join("a.jar").exists());
        assert!(delete(&dir, "../etc/passwd").is_err());
    }

    #[test]
    fn import_sanitizes_names_dedups_and_enforces_extension() {
        let tmp = TempDir::new();
        let dir = tmp.path().join("mods");

        let files = vec![
            ImportFile {
                name: "../../etc/evil.jar".into(),
                bytes: b"one".to_vec(),
            },
            ImportFile {
                name: "evil.jar".into(), // mismo nombre final tras sanear -> debe desduplicarse
                bytes: b"two".to_vec(),
            },
        ];
        let written = import(&dir, files).expect("import");
        assert_eq!(written, vec!["evil.jar", "evil (1).jar"]);
        assert_eq!(fs::read(dir.join("evil.jar")).unwrap_or_default(), b"one");
        assert_eq!(fs::read(dir.join("evil (1).jar")).unwrap_or_default(), b"two");
        assert!(!tmp.path().join("etc").exists());

        let not_jar = vec![ImportFile {
            name: "script.sh".into(),
            bytes: b"x".to_vec(),
        }];
        assert!(import(&dir, not_jar).is_err());

        let too_big = vec![ImportFile {
            name: "huge.jar".into(),
            bytes: vec![0u8; MAX_IMPORT_BYTES + 1],
        }];
        assert!(import(&dir, too_big).is_err());
    }
}
