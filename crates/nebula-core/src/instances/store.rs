use std::{
    fs, io,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

use super::{
    model::{validate_name, INSTANCE_SCHEMA_VERSION},
    Instance, InstanceId, InstanceView, JavaChoice, NewInstance, UpdateInstance,
};
use crate::{
    config::{validate_jvm_args, validate_memory},
    error::CoreError,
    filesystem, NebulaPaths, Result,
};

/// Subcarpetas que existen en toda instancia.
const INSTANCE_SUBDIRS: [&str; 6] = [
    "mods",
    "config",
    "resourcepacks",
    "shaderpacks",
    "saves",
    "logs",
];

#[derive(Debug, Clone, Serialize)]
pub struct BrokenInstance {
    pub id: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstanceList {
    pub instances: Vec<InstanceView>,
    /// Carpetas con ID válido cuyo `instance.json` no se pudo leer.
    pub broken: Vec<BrokenInstance>,
}

/// Acceso a `instances/` en disco. No guarda estado: es barato de clonar.
#[derive(Debug, Clone)]
pub struct InstanceStore {
    paths: NebulaPaths,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl InstanceStore {
    pub fn new(paths: NebulaPaths) -> Self {
        Self { paths }
    }

    fn manifest_path(&self, id: &InstanceId) -> std::path::PathBuf {
        self.paths.instance_dir(id).join("instance.json")
    }

    fn save(&self, instance: &Instance) -> Result<()> {
        let path = self.manifest_path(&instance.id);
        let json = serde_json::to_vec_pretty(instance).map_err(|e| CoreError::Json {
            path: path.clone(),
            source: e,
        })?;
        filesystem::write_atomic(&path, &json)
    }

    fn view(&self, instance: Instance) -> Result<InstanceView> {
        let mods = self.paths.instance_dir(&instance.id).join("mods");
        let mod_count = filesystem::count_files_with_suffix(&mods, &[".jar", ".jar.disabled"])?;
        let installed = crate::minecraft::install::is_installed(
            &self.paths,
            &instance.minecraft_version,
            instance.loader,
            instance.loader_version.as_deref(),
        );
        Ok(InstanceView {
            instance,
            mod_count,
            installed,
        })
    }

    /// Crea la instancia y su árbol de carpetas. El ID es único porque la carpeta
    /// se reserva con `create_dir` (falla si ya existe), sin carreras.
    pub fn create(&self, input: NewInstance) -> Result<InstanceView> {
        let input = input.validated()?;
        filesystem::create_dir_all(&self.paths.instances_dir())?;

        let (id, dir) = {
            let mut found = None;
            for _ in 0..32 {
                let id = InstanceId::generate();
                let dir = self.paths.instance_dir(&id);
                match fs::create_dir(&dir) {
                    Ok(()) => {
                        found = Some((id, dir));
                        break;
                    }
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(CoreError::io(dir, e)),
                }
            }
            found.ok_or_else(|| CoreError::invalid("no se pudo generar un ID único"))?
        };

        let instance = Instance {
            schema_version: INSTANCE_SCHEMA_VERSION,
            id,
            name: input.name,
            minecraft_version: input.minecraft_version,
            loader: input.loader,
            loader_version: input.loader_version,
            java: JavaChoice::Auto,
            memory: input.memory,
            jvm_args: input.jvm_args,
            created_at: now_secs(),
            last_played: None,
            total_playtime_secs: 0,
        };

        let built = INSTANCE_SUBDIRS
            .iter()
            .try_for_each(|sub| filesystem::create_dir_all(&dir.join(sub)))
            .and_then(|()| self.save(&instance));
        if let Err(e) = built {
            let _ = fs::remove_dir_all(&dir); // no dejar instancias a medias
            return Err(e);
        }
        self.view(instance)
    }

    pub fn get(&self, id: &InstanceId) -> Result<InstanceView> {
        self.view(self.load(id)?)
    }

    fn load(&self, id: &InstanceId) -> Result<Instance> {
        let path = self.manifest_path(id);
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(CoreError::NotFound(id.to_string()))
            }
            Err(e) => return Err(CoreError::io(&path, e)),
        };
        let instance: Instance =
            serde_json::from_slice(&bytes).map_err(|e| CoreError::Json { path, source: e })?;
        if instance.id != *id {
            return Err(CoreError::invalid(format!(
                "instance.json de {id} declara otro ID"
            )));
        }
        Ok(instance)
    }

    pub fn list(&self) -> Result<InstanceList> {
        let dir = self.paths.instances_dir();
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(InstanceList {
                    instances: vec![],
                    broken: vec![],
                })
            }
            Err(e) => return Err(CoreError::io(&dir, e)),
        };

        let mut instances = Vec::new();
        let mut broken = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| CoreError::io(&dir, e))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            // Ignora todo lo que no sea una carpeta con ID válido.
            let Ok(id) = InstanceId::parse(&name) else {
                continue;
            };
            if !entry.path().is_dir() {
                continue;
            }
            match self.load(&id).and_then(|i| self.view(i)) {
                Ok(v) => instances.push(v),
                Err(e) => broken.push(BrokenInstance {
                    id: name,
                    error: e.to_string(),
                }),
            }
        }
        instances.sort_by(|a, b| {
            (a.instance.created_at, &a.instance.id).cmp(&(b.instance.created_at, &b.instance.id))
        });
        broken.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(InstanceList { instances, broken })
    }

    pub fn update(&self, id: &InstanceId, patch: UpdateInstance) -> Result<InstanceView> {
        let mut instance = self.load(id)?;
        if let Some(name) = patch.name {
            instance.name = validate_name(&name)?;
        }
        if let Some(memory) = patch.memory {
            validate_memory(memory.min_mb, memory.max_mb)?;
            instance.memory = memory;
        }
        if let Some(args) = patch.jvm_args {
            validate_jvm_args(&args)?;
            instance.jvm_args = args;
        }
        if let Some(java) = patch.java {
            instance.java = java;
        }
        self.save(&instance)?;
        self.view(instance)
    }

    /// Registra el último momento en que se lanzó la instancia.
    pub fn touch_last_played(&self, id: &InstanceId) -> Result<()> {
        let mut instance = self.load(id)?;
        instance.last_played = Some(now_secs());
        self.save(&instance)
    }

    /// Suma `secs` (duración real de la sesión que acaba de terminar) al
    /// tiempo jugado acumulado de la instancia.
    pub fn add_playtime(&self, id: &InstanceId, secs: u64) -> Result<()> {
        let mut instance = self.load(id)?;
        instance.total_playtime_secs = instance.total_playtime_secs.saturating_add(secs);
        self.save(&instance)
    }

    pub fn delete(&self, id: &InstanceId) -> Result<()> {
        let dir = self.paths.instance_dir(id);
        match fs::remove_dir_all(&dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Err(CoreError::NotFound(id.to_string()))
            }
            Err(e) => Err(CoreError::io(dir, e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::{test_util::TempDir, Loader, Memory};

    fn store(tmp: &TempDir) -> InstanceStore {
        let paths = NebulaPaths::new(tmp.path().join("config"), tmp.path().join("data"));
        InstanceStore::new(paths)
    }

    fn new_input(name: &str) -> NewInstance {
        NewInstance {
            name: name.into(),
            minecraft_version: "1.8.9".into(),
            loader: Loader::Fabric,
            loader_version: Some("0.15.11".into()),
            memory: Memory {
                min_mb: 1024,
                max_mb: 2048,
            },
            jvm_args: vec!["-XX:+UseG1GC".into()],
        }
    }

    #[test]
    fn create_builds_full_layout_and_manifest() {
        let tmp = TempDir::new();
        let s = store(&tmp);
        let v = s.create(new_input("PvP")).unwrap();
        let dir = tmp.path().join("data/instances").join(v.instance.id.as_str());
        for sub in INSTANCE_SUBDIRS {
            assert!(dir.join(sub).is_dir(), "{sub}");
        }
        assert!(dir.join("instance.json").is_file());
        assert_eq!(v.mod_count, 0);
        assert_eq!(s.get(&v.instance.id).unwrap().instance, v.instance);
    }

    #[test]
    fn ids_are_unique_across_many_creates() {
        let tmp = TempDir::new();
        let s = store(&tmp);
        let mut ids = std::collections::HashSet::new();
        for i in 0..50 {
            let v = s.create(new_input(&format!("i{i}"))).unwrap();
            assert!(ids.insert(v.instance.id));
        }
        assert_eq!(s.list().unwrap().instances.len(), 50);
    }

    #[test]
    fn update_keeps_id_and_version() {
        let tmp = TempDir::new();
        let s = store(&tmp);
        let before = s.create(new_input("a")).unwrap().instance;
        let after = s
            .update(
                &before.id,
                UpdateInstance {
                    name: Some("  Nuevo nombre ".into()),
                    memory: Some(Memory {
                        min_mb: 2048,
                        max_mb: 4096,
                    }),
                    jvm_args: Some(vec![]),
                    java: None,
                },
            )
            .unwrap()
            .instance;
        assert_eq!(after.id, before.id);
        assert_eq!(after.minecraft_version, before.minecraft_version);
        assert_eq!(after.name, "Nuevo nombre");
        assert_eq!(after.memory.max_mb, 4096);
        assert!(after.jvm_args.is_empty());
    }

    #[test]
    fn delete_removes_and_second_delete_is_not_found() {
        let tmp = TempDir::new();
        let s = store(&tmp);
        let id = s.create(new_input("a")).unwrap().instance.id;
        assert!(s.delete(&id).is_ok());
        assert!(matches!(s.delete(&id), Err(CoreError::NotFound(_))));
        assert!(s.list().unwrap().instances.is_empty());
    }

    #[test]
    fn mod_count_counts_jars_and_disabled() {
        let tmp = TempDir::new();
        let s = store(&tmp);
        let id = s.create(new_input("a")).unwrap().instance.id;
        let mods = tmp.path().join("data/instances").join(id.as_str()).join("mods");
        for f in ["a.jar", "b.jar.disabled", "readme.txt"] {
            fs::write(mods.join(f), b"x").unwrap();
        }
        assert_eq!(s.get(&id).unwrap().mod_count, 2);
    }

    #[test]
    fn broken_manifest_is_reported_not_fatal() {
        let tmp = TempDir::new();
        let s = store(&tmp);
        let good = s.create(new_input("ok")).unwrap().instance.id;
        let bad = s.create(new_input("bad")).unwrap().instance.id;
        let manifest = tmp.path().join("data/instances").join(bad.as_str()).join("instance.json");
        fs::write(manifest, b"basura").unwrap();
        let list = s.list().unwrap();
        assert_eq!(list.instances.len(), 1);
        assert_eq!(list.instances[0].instance.id, good);
        assert_eq!(list.broken.len(), 1);
        assert_eq!(list.broken[0].id, bad.as_str());
    }

    #[test]
    fn validation_rejects_bad_input() {
        let tmp = TempDir::new();
        let s = store(&tmp);
        let mut i = new_input("");
        assert!(s.create(i.clone()).is_err());
        i = new_input("ok");
        i.minecraft_version = "../1.8.9".into();
        assert!(s.create(i).is_err());
        let mut i = new_input("ok");
        i.memory = Memory { min_mb: 4096, max_mb: 1024 };
        assert!(s.create(i).is_err());
        assert!(s.list().unwrap().instances.is_empty());
    }
}
