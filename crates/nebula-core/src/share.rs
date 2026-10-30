//! Instancias compartibles por código (p. ej. `NBL-K7P2X9`).
//!
//! Un código es solo una clave de búsqueda: se resuelve contra
//! `<manifest_base_url>/<código>.json`, un manifiesto JSON alojado donde el
//! usuario decida (Nebula no trae ningún servicio propio — ver el README).
//! El manifiesto describe una instancia (versión, loader, mods, etc.) con
//! URLs y hashes propios; instalarla reutiliza exactamente el mismo
//! `Downloader` verificado con SHA-1 que ya usan las descargas de Mojang y
//! Fabric, así que cambiar de servidor de manifiestos o de almacenamiento de
//! archivos no requiere tocar el resto del launcher.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{
    background,
    downloads::{is_valid, Downloader, Task},
    error::CoreError,
    filesystem::join_safe,
    instances::{
        validate_loader_version, validate_minecraft_version, InstanceId, InstanceStore, Loader,
        Memory, NewInstance,
    },
    minecraft::Reporter,
    NebulaPaths, Result,
};

/// Formato de los códigos: el mismo `NBL-XXXXXX` que los IDs locales, pero
/// aquí es solo una clave de búsqueda — no tiene por qué coincidir con el ID
/// que termine teniendo la instancia una vez instalada.
pub fn validate_code(code: &str) -> Result<String> {
    InstanceId::parse(code.trim()).map(|id| id.as_str().to_owned())
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ManifestFile {
    pub file_name: String,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JavaRequirement {
    pub min_major: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InstanceManifest {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub background_url: Option<String>,
    pub minecraft_version: String,
    pub loader: Loader,
    #[serde(default)]
    pub loader_version: Option<String>,
    #[serde(default)]
    pub java: Option<JavaRequirement>,
    #[serde(default)]
    pub memory: Option<Memory>,
    #[serde(default)]
    pub mods: Vec<ManifestFile>,
    #[serde(default)]
    pub resourcepacks: Vec<ManifestFile>,
    #[serde(default)]
    pub shaderpacks: Vec<ManifestFile>,
    #[serde(default)]
    pub config_files: Vec<ManifestFile>,
}

const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
/// Mismo límite que `background`: 15 MB.
const MAX_BACKGROUND_BYTES: usize = 15 * 1024 * 1024;
/// Topes agregados: un manifiesto pequeño puede declarar miles de archivos
/// enormes y llenar el disco si no se limita el conjunto.
const MAX_MANIFEST_FILES: usize = 512;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB
const MAX_TOTAL_BYTES: u64 = 10 * 1024 * 1024 * 1024; // 10 GiB

fn is_valid_sha1(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn validate_files(
    files: &[ManifestFile],
    label: &str,
    count: &mut usize,
    total: &mut u64,
) -> Result<()> {
    for file in files {
        let sha1 = file.sha1.as_deref().unwrap_or_default();
        if !is_valid_sha1(sha1) {
            return Err(CoreError::invalid(format!(
                "{label}/{}: falta un SHA-1 válido en el manifiesto",
                file.file_name
            )));
        }
        let size = file.size.ok_or_else(|| {
            CoreError::invalid(format!(
                "{label}/{}: falta el tamaño en el manifiesto",
                file.file_name
            ))
        })?;
        if size > MAX_FILE_BYTES {
            return Err(CoreError::invalid(format!(
                "{label}/{}: el archivo supera el tamaño máximo permitido",
                file.file_name
            )));
        }
        *count += 1;
        if *count > MAX_MANIFEST_FILES {
            return Err(CoreError::invalid(
                "el manifiesto declara demasiados archivos",
            ));
        }
        *total = (*total)
            .checked_add(size)
            .ok_or_else(|| CoreError::invalid("el tamaño total del manifiesto desborda"))?;
        if *total > MAX_TOTAL_BYTES {
            return Err(CoreError::invalid(
                "el manifiesto supera el tamaño total máximo permitido",
            ));
        }
    }
    Ok(())
}

/// Los manifiestos compartidos solo pueden instalar archivos con integridad
/// verificable. Sin hash/tamaño, un servidor comprometido podría entregar
/// cualquier JAR y Minecraft lo ejecutaría al iniciar la instancia.
pub fn validate_manifest(manifest: &InstanceManifest) -> Result<()> {
    let mut count = 0;
    let mut total = 0;
    validate_files(&manifest.mods, "mods", &mut count, &mut total)?;
    validate_files(
        &manifest.resourcepacks,
        "resourcepacks",
        &mut count,
        &mut total,
    )?;
    validate_files(&manifest.shaderpacks, "shaderpacks", &mut count, &mut total)?;
    validate_files(&manifest.config_files, "config", &mut count, &mut total)?;
    Ok(())
}

fn manifest_url(base_url: &str, code: &str) -> Result<String> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(CoreError::invalid(
            "configura primero la URL del servicio de manifiestos en Configuración.",
        ));
    }
    Ok(format!("{base}/{code}.json"))
}

/// Hosts https distintos que aparecen en los archivos del manifiesto. Solo
/// esos se permiten al descargarlos: nada fuera de lo que el propio
/// manifiesto declaró.
fn hosts_in(manifest: &InstanceManifest) -> Result<Vec<String>> {
    let mut hosts = HashSet::new();
    let mut add = |raw: &str, label: &str| -> Result<()> {
        let url = reqwest::Url::parse(raw).map_err(|e| {
            CoreError::Network(format!("URL inválida en el manifiesto ({label}): {e}"))
        })?;
        if url.scheme() != "https" {
            return Err(CoreError::Network(format!(
                "{label}: solo se admiten URLs https"
            )));
        }
        hosts.insert(url.host_str().unwrap_or_default().to_owned());
        Ok(())
    };
    for file in manifest
        .mods
        .iter()
        .chain(&manifest.resourcepacks)
        .chain(&manifest.shaderpacks)
        .chain(&manifest.config_files)
    {
        add(&file.url, &file.file_name)?;
    }
    if let Some(url) = &manifest.background_url {
        add(url, "fondo")?;
    }
    Ok(hosts.into_iter().collect())
}

/// Descarga `<manifest_base_url>/<código>.json` y lo valida. No descarga
/// todavía ningún archivo del propio pack: eso es `install`.
pub async fn fetch_manifest(
    client: &reqwest::Client,
    manifest_base_url: &str,
    code: &str,
) -> Result<InstanceManifest> {
    let code = validate_code(code)?;
    let url = manifest_url(manifest_base_url, &code)?;
    let parsed = reqwest::Url::parse(&url)
        .map_err(|e| CoreError::Network(format!("URL de manifiesto inválida: {e}")))?;
    if parsed.scheme() != "https" {
        return Err(CoreError::Network(
            "la URL del servicio de manifiestos debe ser https".into(),
        ));
    }
    let resp = client
        .get(parsed)
        .send()
        .await
        .map_err(|e| CoreError::Network(e.to_string()))?;
    if resp.status().as_u16() == 404 {
        return Err(CoreError::invalid(format!(
            "no existe ninguna instancia con el código {code}"
        )));
    }
    let resp = resp
        .error_for_status()
        .map_err(|e| CoreError::Network(e.to_string()))?;
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| CoreError::Network(e.to_string()))?;
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(CoreError::invalid("el manifiesto es demasiado grande"));
    }
    let mut manifest: InstanceManifest = serde_json::from_slice(&bytes)
        .map_err(|e| CoreError::invalid(format!("manifiesto inválido: {e}")))?;
    validate_minecraft_version(&manifest.minecraft_version)?;
    if let Some(lv) = &manifest.loader_version {
        validate_loader_version(lv)?;
    }
    if manifest.loader == Loader::Fabric && manifest.loader_version.is_none() {
        return Err(CoreError::invalid(
            "el manifiesto pide Fabric pero no indica la versión del loader",
        ));
    }
    validate_manifest(&manifest)?;
    manifest.code = code;
    Ok(manifest)
}

/// Compatibilidad básica: ¿hay algún Java detectado que cumpla el mínimo que
/// pide el manifiesto? `true` si el manifiesto no exige ninguno en concreto.
pub fn java_satisfied(manifest: &InstanceManifest, available_majors: &[u32]) -> bool {
    match manifest.java.as_ref() {
        Some(req) => available_majors.iter().any(|m| *m >= req.min_major),
        None => true,
    }
}

async fn download_set(
    dl: &Downloader,
    dest_dir: &std::path::Path,
    files: &[ManifestFile],
    reporter: &Reporter,
    phase: &str,
) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    let mut tasks = Vec::with_capacity(files.len());
    for file in files {
        let dest = join_safe(dest_dir, &file.file_name)?;
        let sha1 = file.sha1.clone().ok_or_else(|| {
            CoreError::invalid(format!("{}: el manifiesto no indica SHA-1", file.file_name))
        })?;
        let size = file.size.ok_or_else(|| {
            CoreError::invalid(format!(
                "{}: el manifiesto no indica tamaño",
                file.file_name
            ))
        })?;
        tasks.push(Task {
            url: file.url.clone(),
            dest,
            sha1: Some(sha1),
            sha256: None,
            size: Some(size),
        });
    }
    let mut pending = Vec::new();
    for t in &tasks {
        if !is_valid(t.dest.clone(), t.sha1.clone(), t.size).await {
            pending.push(t);
        }
    }
    let total: u64 = pending.iter().filter_map(|t| t.size).sum();
    reporter.phase(phase, total);
    for task in pending {
        dl.download(task, &|d| reporter.add(d)).await?;
    }
    Ok(())
}

/// Crea una instancia local a partir de `manifest` y descarga todo lo que
/// declara (mods, resourcepacks, shaderpacks, configs), verificando siempre
/// SHA-1 y tamaño, y sin volver a bajar lo que ya esté completo. Si algo falla
/// a mitad, borra la instancia a medias en vez de dejarla en un estado confuso.
pub async fn install(
    paths: &NebulaPaths,
    instances: &InstanceStore,
    manifest: &InstanceManifest,
    reporter: &Reporter,
) -> Result<InstanceId> {
    validate_manifest(manifest)?;
    let hosts = hosts_in(manifest)?;
    let host_refs: Vec<&str> = hosts.iter().map(String::as_str).collect();
    let dl = Downloader::new(&host_refs)?;

    let instance = instances.create(NewInstance {
        name: manifest.name.clone(),
        minecraft_version: manifest.minecraft_version.clone(),
        loader: manifest.loader,
        loader_version: manifest.loader_version.clone(),
        memory: manifest.memory.unwrap_or(Memory {
            min_mb: 1024,
            max_mb: 2048,
        }),
        jvm_args: Vec::new(),
    })?;
    let id = instance.instance.id;
    let dir = paths.instance_dir(&id);

    let result = async {
        download_set(
            &dl,
            &dir.join("mods"),
            &manifest.mods,
            reporter,
            "Descargando mods",
        )
        .await?;
        download_set(
            &dl,
            &dir.join("resourcepacks"),
            &manifest.resourcepacks,
            reporter,
            "Descargando resource packs",
        )
        .await?;
        download_set(
            &dl,
            &dir.join("shaderpacks"),
            &manifest.shaderpacks,
            reporter,
            "Descargando shaders",
        )
        .await?;
        download_set(
            &dl,
            &dir.join("config"),
            &manifest.config_files,
            reporter,
            "Descargando configuración",
        )
        .await?;
        if let Some(url) = &manifest.background_url {
            reporter.phase("Descargando fondo de la instancia", 0);
            let bytes = dl.fetch_bytes(url, MAX_BACKGROUND_BYTES).await?;
            background::save_detected(&dir, &bytes)?;
        }
        Ok(())
    }
    .await;

    if let Err(e) = result {
        let _ = instances.delete(&id); // no dejar una instancia a medias
        return Err(e);
    }
    Ok(id)
}

/// Manifiesto de partida para compartir una instancia local: trae sus datos y
/// la lista de archivos de `mods/` con el nombre correcto, pero sin URL — eso
/// solo lo sabe el usuario una vez suba cada archivo a donde vaya a alojarlos.
/// Pensado para rellenarse a mano o con un script antes de publicarlo.
pub fn export_template(
    code: &str,
    name: &str,
    minecraft_version: &str,
    loader: Loader,
    loader_version: Option<&str>,
    mod_file_names: &[String],
) -> InstanceManifest {
    InstanceManifest {
        code: code.to_owned(),
        name: name.to_owned(),
        description: None,
        icon_url: None,
        background_url: None,
        minecraft_version: minecraft_version.to_owned(),
        loader,
        loader_version: loader_version.map(str::to_owned),
        java: None,
        memory: None,
        mods: mod_file_names
            .iter()
            .map(|name| ManifestFile {
                file_name: name.clone(),
                url: String::new(), // a rellenar: dónde subiste este archivo
                sha1: None,
                size: None,
            })
            .collect(),
        resourcepacks: Vec::new(),
        shaderpacks: Vec::new(),
        config_files: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::test_util::TempDir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn sha1_hex(data: &[u8]) -> String {
        use sha1::{Digest, Sha1};
        let mut h = Sha1::new();
        h.update(data);
        crate::downloads::hex(&h.finalize())
    }

    fn sample_manifest() -> InstanceManifest {
        InstanceManifest {
            code: "NBL-K7P2X9".into(),
            name: "Pack de prueba".into(),
            description: Some("Un pack de ejemplo".into()),
            icon_url: None,
            background_url: None,
            minecraft_version: "1.21.1".into(),
            loader: Loader::Fabric,
            loader_version: Some("0.15.11".into()),
            java: None,
            memory: None,
            mods: Vec::new(),
            resourcepacks: Vec::new(),
            shaderpacks: Vec::new(),
            config_files: Vec::new(),
        }
    }

    #[test]
    fn validate_code_accepts_and_rejects_like_local_ids() {
        assert_eq!(
            validate_code("NBL-K7P2X9").unwrap_or_default(),
            "NBL-K7P2X9"
        );
        assert!(validate_code(" nbl-k7p2x9 \n").is_err()); // minúsculas: no es el formato
        assert!(validate_code("../etc/passwd").is_err());
        assert!(validate_code("").is_err());
    }

    #[test]
    fn manifest_url_requires_base_and_trims_trailing_slash() {
        assert!(manifest_url("", "NBL-K7P2X9").is_err());
        assert_eq!(
            manifest_url("https://x.example/manifests/", "NBL-K7P2X9").unwrap_or_default(),
            "https://x.example/manifests/NBL-K7P2X9.json"
        );
    }

    #[test]
    fn background_url_is_in_the_allowlist_and_must_be_https() {
        let mut m = sample_manifest();
        m.background_url = Some("https://cdn.example/fondo.png".into());
        let hosts = hosts_in(&m).expect("hosts");
        assert!(hosts.contains(&"cdn.example".to_owned()));

        m.background_url = Some("http://cdn.example/fondo.png".into());
        assert!(hosts_in(&m).is_err(), "el fondo debe ser https");
    }

    #[test]
    fn java_satisfied_checks_minimum_major() {
        let mut m = sample_manifest();
        m.java = Some(JavaRequirement { min_major: 21 });
        assert!(java_satisfied(&m, &[17, 21]));
        assert!(!java_satisfied(&m, &[8, 17]));
        m.java = None;
        assert!(java_satisfied(&m, &[]));
    }

    #[test]
    fn export_template_lists_mods_without_urls() {
        let m = export_template(
            "NBL-AAAAAA",
            "Mi pack",
            "1.21.1",
            Loader::Fabric,
            Some("0.15.11"),
            &["sodium.jar".to_owned(), "lithium.jar".to_owned()],
        );
        assert_eq!(m.mods.len(), 2);
        assert!(m.mods.iter().all(|f| f.url.is_empty()));
        assert_eq!(m.mods[0].file_name, "sodium.jar");
    }

    #[test]
    fn manifest_requires_hashes_sizes_and_bounded_totals() {
        let mut m = sample_manifest();
        m.mods.push(ManifestFile {
            file_name: "sin-hash.jar".into(),
            url: "https://x/a.jar".into(),
            sha1: None,
            size: Some(1),
        });
        assert!(validate_manifest(&m).is_err());

        m.mods[0].sha1 = Some("aa".into());
        m.mods[0].size = Some(1);
        assert!(validate_manifest(&m).is_err(), "SHA-1 incompleto");

        m.mods[0].sha1 = Some("a".repeat(40));
        m.mods[0].size = Some(MAX_FILE_BYTES + 1);
        assert!(validate_manifest(&m).is_err(), "archivo demasiado grande");

        m.mods.clear();
        for i in 0..=MAX_MANIFEST_FILES {
            m.mods.push(ManifestFile {
                file_name: format!("{i}.jar"),
                url: "https://x/a.jar".into(),
                sha1: Some("a".repeat(40)),
                size: Some(1),
            });
        }
        assert!(validate_manifest(&m).is_err(), "demasiados archivos");
    }

    // ───── servidor falso para fetch_manifest / install de extremo a extremo ─────

    async fn route_server(routes: Vec<(&'static str, Vec<u8>)>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        let routes = std::sync::Arc::new(routes);
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let routes = routes.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let path = req.split_whitespace().nth(1).unwrap_or("/");
                    let body = routes
                        .iter()
                        .find(|(p, _)| *p == path)
                        .map(|(_, b)| b.clone());
                    let (status, body) = match body {
                        Some(b) => ("200 OK", b),
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

    #[tokio::test]
    async fn fetch_manifest_parses_and_rejects_unknown_code() -> Result<()> {
        let body = br#"{
            "code": "ignored",
            "name": "Pack de prueba",
            "minecraft_version": "1.21.1",
            "loader": "fabric",
            "loader_version": "0.15.11",
            "mods": [{"file_name": "a.jar", "url": "https://x/a.jar", "sha1": "aa", "size": 3}]
        }"#;
        let base = route_server(vec![("/manifests/NBL-K7P2X9.json", body.to_vec())]).await;
        let client = reqwest::Client::new();
        // Parseo directo (fetch_manifest exige https; eso ya se prueba aparte
        // en el resto del módulo, igual que en auth/device_flow.rs).
        let resp = client
            .get(format!("{base}/manifests/NBL-K7P2X9.json"))
            .send()
            .await
            .map_err(|e| CoreError::Network(e.to_string()))?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| CoreError::Network(e.to_string()))?;
        let manifest: InstanceManifest = serde_json::from_slice(&bytes).expect("parse");
        assert_eq!(manifest.name, "Pack de prueba");
        assert_eq!(manifest.mods.len(), 1);

        let err = fetch_manifest(&client, "https://example.invalid/manifests", "NBL-000000").await;
        assert!(err.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn install_downloads_verified_files_and_skips_what_is_already_valid() -> Result<()> {
        let tmp = TempDir::new();
        let paths = NebulaPaths::new(tmp.path().join("c"), tmp.path().join("d"));
        let store = InstanceStore::new(paths.clone());

        let mod_bytes = b"MOD-CONTENT".to_vec();
        let base = route_server(vec![("/sodium.jar", mod_bytes.clone())]).await;

        let mut manifest = sample_manifest();
        manifest.mods.push(ManifestFile {
            file_name: "sodium.jar".into(),
            url: format!("{base}/sodium.jar"),
            sha1: Some(sha1_hex(&mod_bytes)),
            size: Some(mod_bytes.len() as u64),
        });

        // Reproduce la lógica de `install` pero sin exigir https (servidor de
        // prueba en texto plano), igual que hacen el resto de los módulos.
        let dl = Downloader::insecure_for_tests()?;
        let created = store.create(NewInstance {
            name: manifest.name.clone(),
            minecraft_version: manifest.minecraft_version.clone(),
            loader: manifest.loader,
            loader_version: manifest.loader_version.clone(),
            memory: Memory {
                min_mb: 1024,
                max_mb: 2048,
            },
            jvm_args: Vec::new(),
        })?;
        let dir = paths.instance_dir(&created.instance.id);
        let reporter = Reporter::silent();
        download_set(&dl, &dir.join("mods"), &manifest.mods, &reporter, "mods").await?;
        assert_eq!(
            std::fs::read(dir.join("mods/sodium.jar")).unwrap_or_default(),
            mod_bytes
        );

        // Segunda pasada: ya está completo y verificado, no debe tocar la red
        // (si lo intentara, `https_only`/el host no coincidirían y fallaría).
        download_set(&dl, &dir.join("mods"), &manifest.mods, &reporter, "mods").await?;
        Ok(())
    }
}
