//! Configuración global de Nebula (`settings.json`).

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{error::CoreError, filesystem, instances::InstanceId, Result};

pub const SETTINGS_SCHEMA_VERSION: u32 = 1;
pub const MIN_RAM_MB: u32 = 512;
pub const MAX_RAM_MB: u32 = 65_536;
const MAX_JVM_ARGS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Dark,
    Black,
    Purple,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundFit {
    Cover,
    Contain,
    Tile,
}

impl Default for BackgroundFit {
    fn default() -> Self {
        Self::Cover
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub theme: Theme,
    pub animations: bool,
    pub transparency: bool,
    /// Color de acento `#RRGGBB`; `None` usa el color del tema.
    pub accent: Option<String>,
    pub background_fit: BackgroundFit,
    /// Desenfoque del fondo del launcher, en píxeles (0-40).
    pub background_blur: u8,
    /// Oscurecimiento del fondo del launcher, en porcentaje (0-90).
    pub background_dim: u8,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            animations: true,
            transparency: true,
            accent: None,
            background_fit: BackgroundFit::Cover,
            background_blur: 0,
            background_dim: 40,
        }
    }
}

impl UiSettings {
    pub fn validate(&self) -> Result<()> {
        if let Some(accent) = &self.accent {
            let valid = accent.len() == 7
                && accent.starts_with('#')
                && accent[1..].chars().all(|c| c.is_ascii_hexdigit());
            if !valid {
                return Err(CoreError::invalid(
                    "el color de acento debe tener el formato #RRGGBB",
                ));
            }
        }
        if self.background_blur > 40 {
            return Err(CoreError::invalid("el desenfoque del fondo debe estar entre 0 y 40"));
        }
        if self.background_dim > 90 {
            return Err(CoreError::invalid("el oscurecimiento del fondo debe estar entre 0 y 90"));
        }
        Ok(())
    }
}

/// Valores por defecto que heredan las instancias nuevas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MinecraftDefaults {
    pub min_ram_mb: u32,
    pub max_ram_mb: u32,
    pub jvm_args: Vec<String>,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
}

impl Default for MinecraftDefaults {
    fn default() -> Self {
        Self {
            min_ram_mb: 1024,
            max_ram_mb: 2048,
            jvm_args: Vec::new(),
            width: 854,
            height: 480,
            fullscreen: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    /// Directorio de datos personalizado. `None` = el XDG por defecto.
    /// Se aplica al reiniciar Nebula.
    pub data_dir: Option<PathBuf>,
    pub selected_instance: Option<InstanceId>,
    pub minecraft: MinecraftDefaults,
    pub ui: UiSettings,
    pub account: AccountSettings,
    pub updates: UpdateSettings,
    pub sharing: SharingSettings,
}

/// Sin `manifest_base_url`, "Importar por código" no tiene dónde buscar: no
/// existe (todavía) un servicio de manifiestos propio. Ver `nebula_core::share`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SharingSettings {
    pub manifest_base_url: Option<String>,
}

/// Ajustes de la cuenta de Microsoft. El Client ID lo registra el propio usuario
/// en Azure (ver el README) — Nebula no trae uno embebido.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AccountSettings {
    pub microsoft_client_id: Option<String>,
}

/// Sin `manifest_url` configurada, Nebula no comprueba nada: no existe (todavía)
/// un feed de actualizaciones propio. Ver `nebula_core::updater`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    pub manifest_url: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            data_dir: None,
            selected_instance: None,
            minecraft: MinecraftDefaults::default(),
            ui: UiSettings::default(),
            account: AccountSettings::default(),
            updates: UpdateSettings::default(),
            sharing: SharingSettings::default(),
        }
    }
}

/// Valida memoria (usado por ajustes e instancias).
pub fn validate_memory(min_mb: u32, max_mb: u32) -> Result<()> {
    if !(MIN_RAM_MB..=MAX_RAM_MB).contains(&min_mb) || !(MIN_RAM_MB..=MAX_RAM_MB).contains(&max_mb)
    {
        return Err(CoreError::invalid(format!(
            "la RAM debe estar entre {MIN_RAM_MB} y {MAX_RAM_MB} MB"
        )));
    }
    if min_mb > max_mb {
        return Err(CoreError::invalid(
            "la RAM mínima no puede superar a la máxima",
        ));
    }
    Ok(())
}

/// Valida argumentos JVM: acotados y sin saltos de línea ni NUL.
pub fn validate_jvm_args(args: &[String]) -> Result<()> {
    if args.len() > MAX_JVM_ARGS {
        return Err(CoreError::invalid(format!(
            "máximo {MAX_JVM_ARGS} argumentos JVM"
        )));
    }
    if args
        .iter()
        .any(|a| a.is_empty() || a.len() > 512 || a.contains(['\0', '\n', '\r']))
    {
        return Err(CoreError::invalid("argumento JVM inválido"));
    }
    Ok(())
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        self.ui.validate()?;
        let m = &self.minecraft;
        validate_memory(m.min_ram_mb, m.max_ram_mb)?;
        validate_jvm_args(&m.jvm_args)?;
        if !(320..=7680).contains(&m.width) || !(240..=4320).contains(&m.height) {
            return Err(CoreError::invalid("resolución fuera de rango"));
        }
        if let Some(dir) = &self.data_dir {
            if !dir.is_absolute() {
                return Err(CoreError::invalid(
                    "el directorio de datos debe ser una ruta absoluta",
                ));
            }
        }
        Ok(())
    }

    /// Carga los ajustes. Si el archivo no existe devuelve los valores por defecto.
    /// Si está corrupto lo conserva como `settings.json.corrupt` (sin perder los
    /// datos del usuario) y arranca con los valores por defecto.
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(CoreError::io(path, e)),
        };
        match serde_json::from_slice::<Settings>(&bytes) {
            Ok(s) if s.validate().is_ok() => Ok(s),
            _ => {
                let mut backup = path.as_os_str().to_os_string();
                backup.push(".corrupt");
                fs::rename(path, &backup).map_err(|e| CoreError::io(path, e))?;
                Ok(Self::default())
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let json = serde_json::to_vec_pretty(self).map_err(|e| CoreError::Json {
            path: path.to_path_buf(),
            source: e,
        })?;
        filesystem::write_atomic(path, &json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::test_util::TempDir;

    #[test]
    fn missing_file_gives_defaults() {
        let tmp = TempDir::new();
        let s = Settings::load(&tmp.path().join("settings.json")).unwrap_or_default();
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn save_then_load_roundtrips() {
        let tmp = TempDir::new();
        let path = tmp.path().join("settings.json");
        let mut s = Settings::default();
        s.ui.theme = Theme::Purple;
        s.ui.accent = Some("#ff00aa".into());
        s.ui.background_fit = BackgroundFit::Tile;
        s.ui.background_blur = 12;
        s.ui.background_dim = 60;
        s.minecraft.max_ram_mb = 4096;
        assert!(s.save(&path).is_ok());
        assert_eq!(Settings::load(&path).unwrap_or_default(), s);
    }

    #[test]
    fn ui_accent_and_effects_are_validated() {
        let mut s = Settings::default();
        s.ui.accent = Some("#8b5cf6".into());
        assert!(s.validate().is_ok());

        s.ui.accent = Some("8b5cf6".into());
        assert!(s.validate().is_err());
        s.ui.accent = Some("#GGGGGG".into());
        assert!(s.validate().is_err());

        s.ui.accent = None;
        s.ui.background_blur = 41;
        assert!(s.validate().is_err());
        s.ui.background_blur = 0;
        s.ui.background_dim = 91;
        assert!(s.validate().is_err());
    }

    #[test]
    fn old_settings_without_new_ui_fields_get_defaults() {
        let tmp = TempDir::new();
        let path = tmp.path().join("settings.json");
        fs::write(
            &path,
            br#"{"schema_version":1,"ui":{"theme":"dark","animations":true,"transparency":true}}"#,
        )
        .unwrap_or_default();
        let s = Settings::load(&path).unwrap_or_default();
        assert_eq!(s.ui.accent, None);
        assert_eq!(s.ui.background_fit, BackgroundFit::Cover);
        assert_eq!(s.ui.background_blur, 0);
        assert_eq!(s.ui.background_dim, 40);
    }

    #[test]
    fn corrupt_file_is_backed_up() {
        let tmp = TempDir::new();
        let path = tmp.path().join("settings.json");
        fs::write(&path, b"{ no es json").unwrap_or_default();
        let s = Settings::load(&path).unwrap_or_default();
        assert_eq!(s, Settings::default());
        assert!(tmp.path().join("settings.json.corrupt").exists());
    }

    #[test]
    fn invalid_values_are_rejected() {
        let mut s = Settings::default();
        s.minecraft.min_ram_mb = 4096;
        s.minecraft.max_ram_mb = 2048;
        assert!(s.validate().is_err());
        let mut s = Settings::default();
        s.data_dir = Some(PathBuf::from("relativa/ruta"));
        assert!(s.validate().is_err());
    }
}
