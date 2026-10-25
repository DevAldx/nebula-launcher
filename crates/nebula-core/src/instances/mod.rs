//! Instancias: instalaciones de Minecraft completamente separadas entre sí.

mod id;
mod model;
mod store;

pub use id::InstanceId;
pub use model::{
    validate_loader_version, validate_minecraft_version, Instance, InstanceView, JavaChoice,
    Loader, Memory, NewInstance, UpdateInstance, INSTANCE_SCHEMA_VERSION,
};
pub use store::{BrokenInstance, InstanceList, InstanceStore};

#[cfg(test)]
pub(crate) mod test_util {
    use std::path::{Path, PathBuf};

    /// Directorio temporal único que se borra al salir de scope.
    pub struct TempDir(PathBuf);

    impl TempDir {
        pub fn new() -> Self {
            let name = format!(
                "nebula-test-{}-{}",
                std::process::id(),
                rand::random::<u64>()
            );
            let path = std::env::temp_dir().join(name);
            std::fs::create_dir_all(&path).unwrap_or_default();
            Self(path)
        }
        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
