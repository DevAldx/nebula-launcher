//! Todo lo específico de Minecraft: metadatos de Mojang, reglas y preparación de archivos.

pub mod fabric;
pub mod install;
pub mod meta;

pub use fabric::{FabricProfile, LoaderVersion};
pub use install::{Installer, PreparedVersion, Progress, Reporter};
pub use meta::{VersionMeta, VersionSummary};
