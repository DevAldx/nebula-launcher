//! Núcleo de Nebula Launcher.
//!
//! Esta crate no depende de Tauri: toda la lógica es testeable por separado y
//! `src-tauri` solo expone comandos IPC finos sobre estas APIs.

pub mod auth;
pub mod background;
pub mod config;
pub mod downloads;
pub mod error;
pub mod filesystem;
pub mod instances;
pub mod java;
pub mod launcher;
pub mod minecraft;
pub mod mods;
pub mod paths;
pub mod share;
pub mod updater;

pub use error::{CoreError, Result};
pub use paths::NebulaPaths;
