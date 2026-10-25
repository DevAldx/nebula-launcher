//! Preparación de una versión de Minecraft: metadatos, cliente, libraries, assets y natives.

use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use futures_util::{stream, StreamExt, TryStreamExt};
use serde::Serialize;

use super::{
    fabric,
    meta::{resolve_libraries, AssetIndex, Manifest, ManifestEntry, ResolvedLib, VersionMeta, VersionSummary},
};
use crate::{
    downloads::{is_valid, Downloader, Task, MOJANG_HOSTS},
    error::CoreError,
    filesystem::{self, join_safe, validate_component},
    instances::{validate_minecraft_version, Loader},
    NebulaPaths, Result,
};

const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const ASSET_BASE_URL: &str = "https://resources.download.minecraft.net";
const MAX_JSON_BYTES: usize = 32 * 1024 * 1024;
const DOWNLOAD_CONCURRENCY: usize = 8;
const VERIFY_CONCURRENCY: usize = 16;
/// Límite por entrada al extraer natives (anti zip-bomb).
const MAX_NATIVE_ENTRY_BYTES: u64 = 256 * 1024 * 1024;

// ───────────────────────────── Progreso ─────────────────────────────

/// `total == 0` significa fase sin porcentaje conocido.
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub phase: String,
    pub done: u64,
    pub total: u64,
}

struct ReporterState {
    phase: Mutex<String>,
    done: AtomicI64,
    total: AtomicI64,
    last_emit: Mutex<Instant>,
}

/// Agrega progreso desde muchas tareas concurrentes y lo emite con límite de frecuencia.
#[derive(Clone)]
pub struct Reporter {
    callback: Arc<dyn Fn(Progress) + Send + Sync>,
    state: Arc<ReporterState>,
}

impl Reporter {
    pub fn new(callback: impl Fn(Progress) + Send + Sync + 'static) -> Self {
        Self {
            callback: Arc::new(callback),
            state: Arc::new(ReporterState {
                phase: Mutex::new(String::new()),
                done: AtomicI64::new(0),
                total: AtomicI64::new(0),
                last_emit: Mutex::new(Instant::now()),
            }),
        }
    }

    pub fn silent() -> Self {
        Self::new(|_| {})
    }

    fn emit(&self) {
        let phase = self.state.phase.lock().map(|p| p.clone()).unwrap_or_default();
        (self.callback)(Progress {
            phase,
            done: self.state.done.load(Ordering::Relaxed).max(0) as u64,
            total: self.state.total.load(Ordering::Relaxed).max(0) as u64,
        });
    }

    pub fn phase(&self, name: &str, total: u64) {
        if let Ok(mut p) = self.state.phase.lock() {
            *p = name.to_owned();
        }
        self.state.done.store(0, Ordering::Relaxed);
        self.state.total.store(total as i64, Ordering::Relaxed);
        self.emit();
    }

    /// Suma (o resta, para revertir) bytes al progreso de la fase actual.
    /// Público porque otros flujos de descarga verificada (compartir
    /// instancias por código) reutilizan este mismo `Reporter`.
    pub fn add(&self, delta: i64) {
        let done = self.state.done.fetch_add(delta, Ordering::Relaxed) + delta;
        let finished = done >= self.state.total.load(Ordering::Relaxed);
        let due = self.state.last_emit.lock().is_ok_and(|mut last| {
            if last.elapsed() >= Duration::from_millis(80) {
                *last = Instant::now();
                true
            } else {
                false
            }
        });
        if due || finished {
            self.emit();
        }
    }
}

// ───────────────────────────── Resultado ─────────────────────────────

/// Todo lo que el lanzador necesita de una versión ya descargada y verificada.
#[derive(Debug, Clone)]
pub struct PreparedVersion {
    pub meta: VersionMeta,
    pub client_jar: PathBuf,
    /// Jars del classpath, en orden (sin el cliente).
    pub classpath_libs: Vec<PathBuf>,
    pub natives_dir: PathBuf,
    pub assets_root: PathBuf,
    pub assets_index_name: String,
    /// Directorio de assets «virtual» (versiones muy antiguas); en otras = `assets_root`.
    pub game_assets_dir: PathBuf,
    pub map_to_resources: bool,
    pub asset_index: AssetIndex,
    /// (ruta del XML de log4j, argumento JVM ya con la ruta sustituida)
    pub logging_arg: Option<String>,
}

pub fn version_dir(paths: &NebulaPaths, version: &str) -> PathBuf {
    paths.versions_dir().join(version)
}

/// ¿Está todo lo necesario ya en disco? (comprobación barata para la UI). Para
/// una instancia Fabric también exige que su perfil propio esté cacheado.
pub fn is_installed(
    paths: &NebulaPaths,
    version: &str,
    loader: Loader,
    loader_version: Option<&str>,
) -> bool {
    if validate_component(version).is_err() {
        return false;
    }
    let dir = version_dir(paths, version);
    let vanilla_ok =
        dir.join(format!("{version}.json")).is_file() && dir.join(format!("{version}.jar")).is_file();
    if !vanilla_ok {
        return false;
    }
    match (loader, loader_version) {
        (Loader::Fabric, Some(lv)) => fabric::profile_cache_path(paths, version, lv)
            .map(|p| p.is_file())
            .unwrap_or(false),
        (Loader::Fabric, None) => false,
        (Loader::Vanilla, _) => true,
    }
}

// ───────────────────────────── Installer ─────────────────────────────

#[derive(Clone)]
pub struct Installer {
    paths: NebulaPaths,
    dl: Downloader,
    manifest_url: String,
    asset_base: String,
}

impl Installer {
    pub fn new(paths: NebulaPaths) -> Result<Self> {
        let hosts: Vec<&str> = MOJANG_HOSTS.iter().chain(fabric::FABRIC_HOSTS).copied().collect();
        Ok(Self {
            paths,
            dl: Downloader::new(&hosts)?,
            manifest_url: MANIFEST_URL.to_owned(),
            asset_base: ASSET_BASE_URL.to_owned(),
        })
    }

    pub async fn fabric_loader_versions(&self, mc_version: &str) -> Result<Vec<fabric::LoaderVersion>> {
        fabric::list_loader_versions(&self.dl, fabric::FABRIC_META_BASE, mc_version).await
    }

    fn manifest_cache(&self) -> PathBuf {
        self.paths.versions_dir().join("version_manifest_v2.json")
    }

    /// Manifiesto oficial; si no hay red usa la copia en caché.
    pub async fn manifest(&self) -> Result<Manifest> {
        match self.dl.fetch_bytes(&self.manifest_url, MAX_JSON_BYTES).await {
            Ok(bytes) => {
                let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|e| {
                    CoreError::Network(format!("manifiesto de versiones inválido: {e}"))
                })?;
                // La caché es solo una optimización: si falla escribirla, no es fatal.
                let _ = filesystem::write_atomic(&self.manifest_cache(), &bytes);
                Ok(manifest)
            }
            Err(net_err) => {
                let cache = self.manifest_cache();
                let bytes = fs::read(&cache).map_err(|_| net_err)?;
                serde_json::from_slice(&bytes).map_err(|e| CoreError::Json {
                    path: cache,
                    source: e,
                })
            }
        }
    }

    pub async fn version_list(&self) -> Result<Vec<VersionSummary>> {
        Ok(self
            .manifest()
            .await?
            .versions
            .into_iter()
            .map(|v| VersionSummary {
                id: v.id,
                kind: v.kind,
                release_time: v.release_time,
            })
            .collect())
    }

    async fn manifest_entry(&self, version: &str) -> Result<ManifestEntry> {
        self.manifest()
            .await?
            .versions
            .into_iter()
            .find(|v| v.id == version)
            .ok_or_else(|| CoreError::invalid(format!("la versión {version} no existe")))
    }

    async fn load_meta(&self, version: &str) -> Result<VersionMeta> {
        let path = version_dir(&self.paths, version).join(format!("{version}.json"));
        if let Ok(bytes) = fs::read(&path) {
            if let Ok(meta) = serde_json::from_slice::<VersionMeta>(&bytes) {
                return Ok(meta);
            }
        }
        let entry = self.manifest_entry(version).await?;
        let task = Task {
            url: entry.url,
            dest: path.clone(),
            sha1: entry.sha1,
            sha256: None,
            size: None,
        };
        self.dl.download(&task, &|_| {}).await?;
        let bytes = fs::read(&path).map_err(|e| CoreError::io(&path, e))?;
        serde_json::from_slice(&bytes).map_err(|e| CoreError::Json { path, source: e })
    }

    /// Metadatos de una versión sin descargar el juego. Lo usa la instalación
    /// de Java al importar una instancia.
    pub async fn version_meta(&self, version: &str) -> Result<VersionMeta> {
        validate_minecraft_version(version)?;
        self.load_meta(version).await
    }

    async fn load_asset_index(&self, meta: &VersionMeta) -> Result<AssetIndex> {
        let id = validate_component(&meta.asset_index.id)?;
        let path = self.paths.assets_dir().join("indexes").join(format!("{id}.json"));
        let task = Task {
            url: meta.asset_index.url.clone(),
            dest: path.clone(),
            sha1: meta.asset_index.sha1.clone(),
            sha256: None,
            size: meta.asset_index.size,
        };
        if !is_valid(path.clone(), task.sha1.clone(), task.size).await {
            self.dl.download(&task, &|_| {}).await?;
        }
        let bytes = fs::read(&path).map_err(|e| CoreError::io(&path, e))?;
        serde_json::from_slice(&bytes).map_err(|e| CoreError::Json { path, source: e })
    }

    /// Descarga y verifica todo lo necesario para ejecutar `version` con el
    /// `loader` indicado (y su `loader_version`, obligatorio si es Fabric).
    /// Idempotente: lo que ya es válido no se vuelve a descargar.
    pub async fn prepare(
        &self,
        version: &str,
        loader: Loader,
        loader_version: Option<&str>,
        reporter: &Reporter,
    ) -> Result<PreparedVersion> {
        validate_minecraft_version(version)?;
        reporter.phase("Leyendo metadatos de la versión", 0);
        let vanilla_meta = self.load_meta(version).await?;
        if vanilla_meta.inherits_from.is_some() {
            return Err(CoreError::Launch(
                "el JSON de esta versión ya hereda de otra; no se admite anidar loaders".into(),
            ));
        }
        let meta = match (loader, loader_version) {
            (Loader::Fabric, Some(loader_version)) => {
                reporter.phase("Leyendo metadatos de Fabric", 0);
                let profile = fabric::fetch_profile(
                    &self.dl,
                    fabric::FABRIC_META_BASE,
                    &self.paths,
                    version,
                    loader_version,
                )
                .await?;
                fabric::merge_with_fabric(vanilla_meta, profile)
            }
            (Loader::Fabric, None) => {
                return Err(CoreError::invalid(
                    "a esta instancia le falta la versión de Fabric Loader",
                ))
            }
            (Loader::Vanilla, _) => vanilla_meta,
        };
        let index = self.load_asset_index(&meta).await?;
        let features = HashSet::new();
        let libs = resolve_libraries(&meta, &features)?;

        let vdir = version_dir(&self.paths, version);
        let client_jar = vdir.join(format!("{version}.jar"));
        let natives_dir = vdir.join("natives");
        let libs_root = self.paths.libraries_dir();

        // ── plan de descargas ──
        let mut tasks: Vec<Task> = vec![Task {
            url: meta.downloads.client.url.clone(),
            dest: client_jar.clone(),
            sha1: meta.downloads.client.sha1.clone(),
            sha256: None,
            size: meta.downloads.client.size,
        }];
        let mut classpath_libs = Vec::new();
        let mut seen_paths: HashSet<String> = HashSet::new();
        for lib in &libs {
            for file in [&lib.artifact, &lib.native].into_iter().flatten() {
                let dest = join_safe(&libs_root, &file.rel_path)?;
                if seen_paths.insert(file.rel_path.clone()) {
                    tasks.push(Task {
                        url: file.url.clone(),
                        dest,
                        sha1: file.sha1.clone(),
                        sha256: None,
                        size: file.size,
                    });
                }
            }
            if let Some(a) = &lib.artifact {
                classpath_libs.push(join_safe(&libs_root, &a.rel_path)?);
            }
        }

        let mut logging_arg = None;
        if let Some(client) = meta.logging.as_ref().and_then(|l| l.client.as_ref()) {
            let id = validate_component(&client.file.id)?;
            let dest = self.paths.assets_dir().join("log_configs").join(id);
            tasks.push(Task {
                url: client.file.url.clone(),
                dest: dest.clone(),
                sha1: client.file.sha1.clone(),
                sha256: None,
                size: client.file.size,
            });
            logging_arg = Some(client.argument.replace("${path}", &dest.to_string_lossy()));
        }

        let objects_root = self.paths.assets_dir().join("objects");
        let mut seen_hashes = HashSet::new();
        for obj in index.objects.values() {
            let hash = obj.hash.to_ascii_lowercase();
            if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(CoreError::Integrity(format!("hash de asset inválido: {hash}")));
            }
            if !seen_hashes.insert(hash.clone()) {
                continue;
            }
            let prefix = &hash[..2];
            tasks.push(Task {
                url: format!("{}/{prefix}/{hash}", self.asset_base),
                dest: objects_root.join(prefix).join(&hash),
                sha1: Some(hash),
                sha256: None,
                size: Some(obj.size),
            });
        }

        // ── verificación (hash de lo que ya existe) ──
        reporter.phase("Verificando archivos", 0);
        let pending: Vec<Task> = stream::iter(tasks)
            .map(|t| async move {
                if is_valid(t.dest.clone(), t.sha1.clone(), t.size).await {
                    None
                } else {
                    Some(t)
                }
            })
            .buffer_unordered(VERIFY_CONCURRENCY)
            .filter_map(|t| async move { t })
            .collect()
            .await;

        // ── descarga ──
        if !pending.is_empty() {
            let total: u64 = pending.iter().filter_map(|t| t.size).sum();
            reporter.phase("Descargando Minecraft", total);
            let rep = reporter.clone();
            let on_bytes = move |d: i64| rep.add(d);
            stream::iter(pending.into_iter().map(Ok::<_, CoreError>))
                .try_for_each_concurrent(DOWNLOAD_CONCURRENCY, |task| {
                    let dl = self.dl.clone();
                    let on_bytes = &on_bytes;
                    async move { dl.download(&task, on_bytes).await }
                })
                .await?;
        }

        // ── natives y assets virtuales ──
        reporter.phase("Extrayendo librerías nativas", 0);
        extract_natives(&libs, &libs_root, &natives_dir).await?;

        let assets_root = self.paths.assets_dir();
        let assets_index_name = meta.asset_index.id.clone();
        let game_assets_dir = if index.r#virtual {
            reporter.phase("Preparando assets", 0);
            let dir = assets_root.join("virtual").join(&assets_index_name);
            copy_assets(&index, &objects_root, &dir).await?;
            dir
        } else {
            assets_root.clone()
        };

        Ok(PreparedVersion {
            map_to_resources: index.map_to_resources,
            asset_index: index,
            meta,
            client_jar,
            classpath_libs,
            natives_dir,
            assets_root,
            assets_index_name,
            game_assets_dir,
            logging_arg,
        })
    }
}

/// Copia (sin sobrescribir lo ya válido) los assets del índice a `dest/<nombre>`.
/// Se usa para `virtual` (assets/virtual/<id>) y `map_to_resources` (<instancia>/resources).
pub async fn copy_assets(index: &AssetIndex, objects_root: &Path, dest: &Path) -> Result<()> {
    let objects: HashMap<String, (String, u64)> = index
        .objects
        .iter()
        .map(|(name, o)| (name.clone(), (o.hash.to_ascii_lowercase(), o.size)))
        .collect();
    let objects_root = objects_root.to_path_buf();
    let dest = dest.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<()> {
        for (name, (hash, size)) in objects {
            let target = join_safe(&dest, &name)?;
            if fs::metadata(&target).is_ok_and(|m| m.len() == size) {
                continue;
            }
            let source = objects_root.join(&hash[..2]).join(&hash);
            if let Some(parent) = target.parent() {
                filesystem::create_dir_all(parent)?;
            }
            fs::copy(&source, &target).map_err(|e| CoreError::io(&target, e))?;
        }
        Ok(())
    })
    .await
    .map_err(|e| CoreError::Launch(e.to_string()))?
}

async fn extract_natives(libs: &[ResolvedLib], libs_root: &Path, natives_dir: &Path) -> Result<()> {
    let jobs: Vec<(PathBuf, Vec<String>)> = libs
        .iter()
        .filter_map(|l| {
            let file = l.native.as_ref()?;
            let jar = join_safe(libs_root, &file.rel_path).ok()?;
            Some((jar, l.extract_exclude.clone()))
        })
        .collect();
    let natives_dir = natives_dir.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<()> {
        filesystem::create_dir_all(&natives_dir)?;
        for (jar, exclude) in jobs {
            extract_zip(&jar, &natives_dir, &exclude)?;
        }
        Ok(())
    })
    .await
    .map_err(|e| CoreError::Launch(e.to_string()))?
}

/// Extrae un zip a `out_dir` evitando zip-slip (`join_safe`) y entradas gigantes.
pub fn extract_zip(zip_path: &Path, out_dir: &Path, exclude: &[String]) -> Result<()> {
    let file = fs::File::open(zip_path).map_err(|e| CoreError::io(zip_path, e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| CoreError::Integrity(format!("{}: {e}", zip_path.display())))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| CoreError::Integrity(format!("{}: {e}", zip_path.display())))?;
        let name = entry.name().to_owned();
        if entry.is_dir() || exclude.iter().any(|x| name.starts_with(x.as_str())) {
            continue;
        }
        let target = join_safe(out_dir, &name)?;
        if let Some(parent) = target.parent() {
            filesystem::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(&target).map_err(|e| CoreError::io(&target, e))?;
        let copied = io::copy(&mut entry.by_ref().take(MAX_NATIVE_ENTRY_BYTES + 1), &mut out)
            .map_err(|e| CoreError::io(&target, e))?;
        if copied > MAX_NATIVE_ENTRY_BYTES {
            return Err(CoreError::Integrity(format!("{name}: entrada demasiado grande")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::test_util::TempDir;
    use std::io::Write;

    fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = fs::File::create(path).expect("create zip");
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            zip.start_file(*name, opts).expect("start");
            zip.write_all(data).expect("write");
        }
        zip.finish().expect("finish");
    }

    #[test]
    fn extract_zip_skips_excluded_and_blocks_zip_slip() {
        let tmp = TempDir::new();
        let zip_path = tmp.path().join("n.zip");
        make_zip(
            &zip_path,
            &[
                ("liblwjgl.so", b"so"),
                ("sub/libx.so", b"x"),
                ("META-INF/MANIFEST.MF", b"m"),
            ],
        );
        let out = tmp.path().join("out");
        let exclude = vec!["META-INF/".to_owned()];
        assert!(extract_zip(&zip_path, &out, &exclude).is_ok());
        assert!(out.join("liblwjgl.so").is_file());
        assert!(out.join("sub/libx.so").is_file());
        assert!(!out.join("META-INF").exists());

        let evil = tmp.path().join("evil.zip");
        make_zip(&evil, &[("../escape.so", b"boom")]);
        assert!(extract_zip(&evil, &out, &[]).is_err());
        assert!(!tmp.path().join("escape.so").exists());
    }

    #[test]
    fn is_installed_checks_both_files_and_rejects_bad_ids() {
        let tmp = TempDir::new();
        let paths = NebulaPaths::new(tmp.path().join("c"), tmp.path().join("d"));
        assert!(!is_installed(&paths, "1.8.9", Loader::Vanilla, None));
        let dir = version_dir(&paths, "1.8.9");
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join("1.8.9.json"), b"{}").expect("json");
        assert!(!is_installed(&paths, "1.8.9", Loader::Vanilla, None));
        fs::write(dir.join("1.8.9.jar"), b"j").expect("jar");
        assert!(is_installed(&paths, "1.8.9", Loader::Vanilla, None));
        assert!(!is_installed(&paths, "../x", Loader::Vanilla, None));
        // Fabric: también exige que el perfil propio esté cacheado.
        assert!(!is_installed(&paths, "1.8.9", Loader::Fabric, Some("0.15.11")));
        let fdir = version_dir(&paths, "fabric-loader-0.15.11-1.8.9");
        fs::create_dir_all(&fdir).expect("fdir");
        fs::write(fdir.join("fabric-loader-0.15.11-1.8.9.json"), b"{}").expect("fjson");
        assert!(is_installed(&paths, "1.8.9", Loader::Fabric, Some("0.15.11")));
        assert!(!is_installed(&paths, "1.8.9", Loader::Fabric, None));
    }

    #[test]
    fn reporter_aggregates_and_reverts() {
        let last = Arc::new(Mutex::new(None::<Progress>));
        let sink = last.clone();
        let r = Reporter::new(move |p| {
            if let Ok(mut g) = sink.lock() {
                *g = Some(p);
            }
        });
        r.phase("Descargando", 100);
        r.add(60);
        r.add(-20);
        r.add(60); // done = 100 = total → emite siempre
        let got = last.lock().ok().and_then(|g| g.clone()).expect("progress");
        assert_eq!((got.done, got.total), (100, 100));
    }

    // ───── prepare() de punta a punta contra un «Mojang» falso en local ─────

    use std::sync::atomic::AtomicUsize;
    use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpListener};

    fn sha1_hex(data: &[u8]) -> String {
        use sha1::{Digest, Sha1};
        let mut h = Sha1::new();
        h.update(data);
        crate::downloads::hex(&h.finalize())
    }

    #[tokio::test]
    async fn prepare_downloads_verifies_extracts_and_is_idempotent() -> Result<()> {
        let tmp = TempDir::new();
        let paths = NebulaPaths::new(tmp.path().join("c"), tmp.path().join("d"));

        // Contenidos remotos.
        let client = b"CLIENT-JAR".to_vec();
        let plain_lib = b"PLAIN-LIB".to_vec();
        let native_zip_path = tmp.path().join("native.zip");
        make_zip(&native_zip_path, &[("liblwjgl.so", b"SO"), ("META-INF/X", b"x")]);
        let native_zip = fs::read(&native_zip_path).expect("zip bytes");
        let asset = b"ASSET-DATA".to_vec();
        let asset_hash = sha1_hex(&asset);
        let index = format!(
            r#"{{"objects":{{"icons/icon.png":{{"hash":"{asset_hash}","size":{}}}}}}}"#,
            asset.len()
        )
        .into_bytes();

        // El servidor necesita su propia URL base dentro de los JSON: reservamos el puerto primero.
        let probe = TcpListener::bind("127.0.0.1:0").await.expect("probe");
        let addr = probe.local_addr().expect("addr");
        drop(probe);
        let base = format!("http://{addr}");

        let version_json = format!(
            r#"{{
              "id":"9.9.9","type":"release","releaseTime":"2015-01-01T00:00:00+00:00",
              "mainClass":"Main","minecraftArguments":"--username ${{auth_player_name}}",
              "assetIndex":{{"id":"idx","sha1":"{idx_sha}","size":{idx_len},"url":"{base}/idx.json"}},
              "downloads":{{"client":{{"sha1":"{c_sha}","size":{c_len},"url":"{base}/client.jar"}}}},
              "libraries":[
                {{"name":"a:plain:1","downloads":{{"artifact":{{"path":"a/plain/1/plain-1.jar","sha1":"{p_sha}","size":{p_len},"url":"{base}/plain.jar"}}}}}},
                {{"name":"a:nat:1","natives":{{"linux":"natives-linux"}},"extract":{{"exclude":["META-INF/"]}},
                  "downloads":{{"classifiers":{{"natives-linux":{{"path":"a/nat/1/nat-1-natives-linux.jar","sha1":"{n_sha}","size":{n_len},"url":"{base}/nat.jar"}}}}}}}}
              ]
            }}"#,
            idx_sha = sha1_hex(&index), idx_len = index.len(),
            c_sha = sha1_hex(&client), c_len = client.len(),
            p_sha = sha1_hex(&plain_lib), p_len = plain_lib.len(),
            n_sha = sha1_hex(&native_zip), n_len = native_zip.len(),
        )
        .into_bytes();
        let manifest = format!(
            r#"{{"versions":[{{"id":"9.9.9","type":"release","url":"{base}/9.9.9.json","sha1":"{}"}}]}}"#,
            sha1_hex(&version_json)
        )
        .into_bytes();

        let mut routes = HashMap::new();
        routes.insert("/manifest.json".to_owned(), manifest);
        routes.insert("/9.9.9.json".to_owned(), version_json);
        routes.insert("/idx.json".to_owned(), index);
        routes.insert("/client.jar".to_owned(), client);
        routes.insert("/plain.jar".to_owned(), plain_lib);
        routes.insert("/nat.jar".to_owned(), native_zip);
        routes.insert(format!("/{}/{asset_hash}", &asset_hash[..2]), asset);

        let hits = Arc::new(AtomicUsize::new(0));
        let served_base = fake_server_on(addr, routes, hits.clone()).await;
        assert_eq!(served_base, base);

        let installer = Installer {
            paths: paths.clone(),
            dl: Downloader::insecure_for_tests()?,
            manifest_url: format!("{base}/manifest.json"),
            asset_base: base.clone(),
        };

        let phases = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = phases.clone();
        let reporter = Reporter::new(move |p| {
            if let Ok(mut g) = sink.lock() {
                if g.last() != Some(&p.phase) {
                    g.push(p.phase);
                }
            }
        });
        let prepared = installer.prepare("9.9.9", Loader::Vanilla, None, &reporter).await?;

        assert_eq!(fs::read(&prepared.client_jar).unwrap_or_default(), b"CLIENT-JAR");
        assert_eq!(prepared.classpath_libs.len(), 1);
        assert_eq!(fs::read(&prepared.classpath_libs[0]).unwrap_or_default(), b"PLAIN-LIB");
        assert_eq!(fs::read(prepared.natives_dir.join("liblwjgl.so")).unwrap_or_default(), b"SO");
        assert!(!prepared.natives_dir.join("META-INF").exists());
        assert!(paths.assets_dir().join("objects").join(&asset_hash[..2]).join(&asset_hash).is_file());
        assert!(is_installed(&paths, "9.9.9", Loader::Vanilla, None));
        let seen = phases.lock().map(|g| g.clone()).unwrap_or_default();
        assert!(seen.contains(&"Descargando Minecraft".to_owned()), "{seen:?}");

        // Segunda ejecución: todo válido en disco → cero peticiones de red.
        let before = hits.load(Ordering::SeqCst);
        installer.prepare("9.9.9", Loader::Vanilla, None, &Reporter::silent()).await?;
        assert_eq!(hits.load(Ordering::SeqCst), before, "no debe volver a descargar");

        // Un archivo corrupto se detecta por hash y se vuelve a bajar.
        fs::write(&prepared.client_jar, b"CORRUPTO").expect("corrupt");
        installer.prepare("9.9.9", Loader::Vanilla, None, &Reporter::silent()).await?;
        assert_eq!(fs::read(&prepared.client_jar).unwrap_or_default(), b"CLIENT-JAR");
        Ok(())
    }

    async fn fake_server_on(
        addr: std::net::SocketAddr,
        routes: HashMap<String, Vec<u8>>,
        hits: Arc<AtomicUsize>,
    ) -> String {
        let listener = TcpListener::bind(addr).await.expect("bind addr");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        let routes = Arc::new(routes);
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else { return };
                let routes = routes.clone();
                let hits = hits.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let path = req.split_whitespace().nth(1).unwrap_or("/").to_owned();
                    hits.fetch_add(1, Ordering::SeqCst);
                    let (status, body) = match routes.get(&path) {
                        Some(b) => ("200 OK", b.clone()),
                        None => ("404 Not Found", Vec::new()),
                    };
                    let head = format!(
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(&body).await;
                });
            }
        });
        base
    }
}
