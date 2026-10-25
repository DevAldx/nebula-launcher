use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::InstanceId;
use crate::{
    config::{validate_jvm_args, validate_memory},
    error::CoreError,
    Result,
};

pub const INSTANCE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    Vanilla,
    Fabric,
}

/// Qué Java usa la instancia. `Auto` se resuelve al lanzar (Fase 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "path", rename_all = "lowercase")]
pub enum JavaChoice {
    Auto,
    Path(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    pub min_mb: u32,
    pub max_mb: u32,
}

/// Contenido de `instance.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instance {
    pub schema_version: u32,
    /// Inmutable durante toda la vida de la instancia.
    pub id: InstanceId,
    pub name: String,
    pub minecraft_version: String,
    pub loader: Loader,
    /// Versión de Fabric Loader resuelta al crear la instancia. `Some` solo
    /// cuando `loader` es `Fabric`; se fija una vez para que la instancia no
    /// cambie de loader por su cuenta si Fabric publica una build más nueva.
    pub loader_version: Option<String>,
    pub java: JavaChoice,
    pub memory: Memory,
    pub jvm_args: Vec<String>,
    /// Segundos Unix.
    pub created_at: u64,
    pub last_played: Option<u64>,
    /// Suma de segundos jugados, medidos de verdad al cerrar cada sesión.
    /// `default` para que las instancias creadas antes de este campo sigan
    /// cargando bien (se quedan en 0 hasta la próxima partida).
    #[serde(default)]
    pub total_playtime_secs: u64,
}

/// Lo que ve el frontend: la instancia más datos derivados del disco.
#[derive(Debug, Clone, Serialize)]
pub struct InstanceView {
    #[serde(flatten)]
    pub instance: Instance,
    pub mod_count: usize,
    /// Metadatos y jar de cliente ya descargados (derivado del disco).
    pub installed: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewInstance {
    pub name: String,
    pub minecraft_version: String,
    pub loader: Loader,
    /// Resuelto por el backend (última build estable de Fabric) antes de
    /// llamar a `InstanceStore::create`; el frontend no lo rellena.
    #[serde(default)]
    pub loader_version: Option<String>,
    pub memory: Memory,
    pub jvm_args: Vec<String>,
}

/// Campos editables. La versión de Minecraft y el loader no cambian aquí.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateInstance {
    pub name: Option<String>,
    pub memory: Option<Memory>,
    pub jvm_args: Option<Vec<String>>,
    pub java: Option<JavaChoice>,
}

pub(super) fn validate_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 48 || name.chars().any(char::is_control) {
        return Err(CoreError::invalid(
            "el nombre debe tener entre 1 y 48 caracteres",
        ));
    }
    Ok(name.to_owned())
}

/// Versiones tipo `1.8.9`, `1.21.1`, `24w14a`, `1.14_combat-0`.
pub fn validate_minecraft_version(v: &str) -> Result<String> {
    let v = v.trim();
    let ok = !v.is_empty()
        && v.len() <= 32
        && v.starts_with(|c: char| c.is_ascii_alphanumeric())
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(v.to_owned())
    } else {
        Err(CoreError::invalid("versión de Minecraft inválida"))
    }
}

/// Versiones de Fabric Loader tipo `0.15.11`, `0.16.0-beta.1`.
pub fn validate_loader_version(v: &str) -> Result<String> {
    let v = v.trim();
    let ok = !v.is_empty()
        && v.len() <= 32
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(v.to_owned())
    } else {
        Err(CoreError::invalid("versión de Fabric Loader inválida"))
    }
}

impl NewInstance {
    pub(super) fn validated(self) -> Result<Self> {
        validate_memory(self.memory.min_mb, self.memory.max_mb)?;
        validate_jvm_args(&self.jvm_args)?;
        let loader_version = match (self.loader, &self.loader_version) {
            (Loader::Fabric, Some(v)) => Some(validate_loader_version(v)?),
            (Loader::Fabric, None) => {
                return Err(CoreError::invalid(
                    "falta la versión de Fabric Loader para esta instancia",
                ))
            }
            (Loader::Vanilla, None) => None,
            (Loader::Vanilla, Some(_)) => {
                return Err(CoreError::invalid(
                    "una instancia vanilla no debe tener versión de loader",
                ))
            }
        };
        Ok(Self {
            name: validate_name(&self.name)?,
            minecraft_version: validate_minecraft_version(&self.minecraft_version)?,
            loader_version,
            ..self
        })
    }
}
