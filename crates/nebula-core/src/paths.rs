use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use crate::{error::CoreError, filesystem, instances::InstanceId, Result};

/// Ubicaciones de Nebula siguiendo XDG en Linux.
///
/// * `config_dir` (`~/.config/nebula`): solo `settings.json`. Vive aparte para que
///   el directorio de datos pueda cambiarse sin problema de huevo y gallina.
/// * `data_dir` (`~/.local/share/nebula`): instancias y almacenamiento compartido.
#[derive(Debug, Clone)]
pub struct NebulaPaths {
    config_dir: PathBuf,
    data_dir: PathBuf,
}

impl NebulaPaths {
    pub fn from_system() -> Result<Self> {
        let dirs = ProjectDirs::from("dev", "nebula", "nebula").ok_or(CoreError::NoHomeDir)?;
        Ok(Self {
            config_dir: dirs.config_dir().to_path_buf(),
            data_dir: dirs.data_dir().to_path_buf(),
        })
    }

    pub fn new(config_dir: impl Into<PathBuf>, data_dir: impl Into<PathBuf>) -> Self {
        Self {
            config_dir: config_dir.into(),
            data_dir: data_dir.into(),
        }
    }

    /// Devuelve una copia con otro directorio de datos (debe ser absoluto).
    pub fn with_data_dir(&self, data_dir: &Path) -> Result<Self> {
        if !data_dir.is_absolute() {
            return Err(CoreError::invalid(
                "el directorio de datos debe ser una ruta absoluta",
            ));
        }
        Ok(Self {
            config_dir: self.config_dir.clone(),
            data_dir: data_dir.to_path_buf(),
        })
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }
    pub fn instances_dir(&self) -> PathBuf {
        self.data_dir.join("instances")
    }
    pub fn assets_dir(&self) -> PathBuf {
        self.data_dir.join("assets")
    }
    pub fn libraries_dir(&self) -> PathBuf {
        self.data_dir.join("libraries")
    }
    pub fn versions_dir(&self) -> PathBuf {
        self.data_dir.join("versions")
    }
    pub fn java_dir(&self) -> PathBuf {
        self.data_dir.join("java")
    }
    pub fn accounts_dir(&self) -> PathBuf {
        self.data_dir.join("accounts")
    }

    /// Ruta de una instancia. Es segura por construcción: `InstanceId` solo admite
    /// `NBL-[A-Z0-9]{6}`, así que no puede escapar de `instances/`.
    pub fn instance_dir(&self, id: &InstanceId) -> PathBuf {
        self.instances_dir().join(id.as_str())
    }

    /// Crea el árbol base de directorios compartidos.
    pub fn ensure_layout(&self) -> Result<()> {
        for dir in [
            &self.config_dir,
            &self.data_dir,
            &self.instances_dir(),
            &self.assets_dir(),
            &self.libraries_dir(),
            &self.versions_dir(),
            &self.java_dir(),
            &self.accounts_dir(),
        ] {
            filesystem::create_dir_all(dir)?;
        }
        Ok(())
    }
}
