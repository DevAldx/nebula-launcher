//! Soporte para Fabric: versiones del loader, perfil de lanzamiento y su fusión
//! con el JSON vanilla de la versión base (`inheritsFrom`).
//!
//! Fabric publica su propio JSON de lanzamiento en <https://meta.fabricmc.net>,
//! con el mismo formato que usa el launcher oficial de Mojang para versiones que
//! heredan de otra (`inheritsFrom`). El perfil de Fabric casi nunca redefine
//! `assetIndex`, `downloads` ni `javaVersion`: esos siguen viniendo del JSON
//! vanilla. Lo que aporta es su propio `mainClass` y unas pocas `libraries`
//! (el propio Fabric Loader + el mapeo "intermediary"), que se añaden a las de
//! la versión vanilla.

use serde::Deserialize;

use super::meta::{Library, VersionMeta};
use crate::{
    downloads::Downloader, error::CoreError, filesystem::validate_component,
    instances::validate_minecraft_version, NebulaPaths, Result,
};

pub const FABRIC_HOSTS: &[&str] = &["meta.fabricmc.net", "maven.fabricmc.net"];
/// Base real de Fabric Meta. Un parámetro `meta_base` aparte (en vez de esta
/// constante a secas) es lo que permite apuntar las pruebas a un servidor local.
pub const FABRIC_META_BASE: &str = "https://meta.fabricmc.net/v2";
const MAX_JSON_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct LoaderVersion {
    pub version: String,
    pub stable: bool,
}

#[derive(Deserialize)]
struct LoaderBuild {
    version: String,
    #[serde(default)]
    stable: bool,
}

#[derive(Deserialize)]
struct LoaderEntry {
    loader: LoaderBuild,
}

/// Compilaciones de Fabric Loader disponibles para una versión de Minecraft,
/// más recientes primero (tal como las devuelve Fabric). `meta_base` es
/// `FABRIC_META_BASE` en producción; las pruebas pasan un servidor local.
pub async fn list_loader_versions(
    dl: &Downloader,
    meta_base: &str,
    mc_version: &str,
) -> Result<Vec<LoaderVersion>> {
    validate_minecraft_version(mc_version)?;
    let url = format!("{meta_base}/versions/loader/{mc_version}");
    let bytes = dl.fetch_bytes(&url, MAX_JSON_BYTES).await?;
    let entries: Vec<LoaderEntry> = serde_json::from_slice(&bytes)
        .map_err(|e| CoreError::Network(format!("respuesta de Fabric Meta inesperada: {e}")))?;
    Ok(entries
        .into_iter()
        .map(|e| LoaderVersion {
            version: e.loader.version,
            stable: e.loader.stable,
        })
        .collect())
}

/// La última compilación marcada `stable`; si ninguna lo está (versión de
/// Minecraft muy nueva), la más reciente de la lista.
pub fn pick_stable(versions: &[LoaderVersion]) -> Result<&str> {
    versions
        .iter()
        .find(|v| v.stable)
        .or_else(|| versions.first())
        .map(|v| v.version.as_str())
        .ok_or_else(|| {
            CoreError::invalid("Fabric no tiene ninguna compilación para esta versión de Minecraft")
        })
}

#[derive(Debug, Clone, Default, Deserialize)]
struct FabricArguments {
    #[serde(default)]
    game: Vec<super::meta::Arg>,
    #[serde(default)]
    jvm: Vec<super::meta::Arg>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FabricProfile {
    pub id: String,
    #[serde(rename = "mainClass")]
    main_class: String,
    #[serde(default)]
    arguments: FabricArguments,
    #[serde(default)]
    libraries: Vec<Library>,
}

pub(super) fn profile_cache_path(
    paths: &NebulaPaths,
    mc_version: &str,
    loader_version: &str,
) -> Result<std::path::PathBuf> {
    validate_minecraft_version(mc_version)?;
    let loader_version = validate_component(loader_version)?;
    // El id de Fabric (p. ej. "fabric-loader-0.15.11-1.21.1") es también un
    // identificador de versión válido para `versions/`, así que se cachea igual
    // que una versión vanilla: una carpeta propia bajo `versions/`.
    let dir_name = format!("fabric-loader-{loader_version}-{mc_version}");
    Ok(super::install::version_dir(paths, &dir_name).join(format!("{dir_name}.json")))
}

/// Descarga (o reutiliza del caché en disco) el perfil de lanzamiento de Fabric.
pub async fn fetch_profile(
    dl: &Downloader,
    meta_base: &str,
    paths: &NebulaPaths,
    mc_version: &str,
    loader_version: &str,
) -> Result<FabricProfile> {
    let cache_path = profile_cache_path(paths, mc_version, loader_version)?;
    if let Some(bytes) = crate::filesystem::read_verified(&cache_path) {
        if let Ok(profile) = serde_json::from_slice::<FabricProfile>(&bytes) {
            return Ok(profile);
        }
    }
    let url = format!("{meta_base}/versions/loader/{mc_version}/{loader_version}/profile/json");
    let bytes = dl.fetch_bytes(&url, MAX_JSON_BYTES).await?;
    let profile: FabricProfile = serde_json::from_slice(&bytes)
        .map_err(|e| CoreError::Network(format!("perfil de Fabric inesperado: {e}")))?;
    // Solo caché: si falla escribirla no es un error fatal.
    let _ = crate::filesystem::write_verified(&cache_path, &bytes);
    Ok(profile)
}

/// Fusiona el perfil de Fabric sobre el JSON vanilla ya resuelto, siguiendo las
/// reglas de `inheritsFrom`: los campos propios de Fabric (`mainClass`) sustituyen
/// a los de vanilla, y `libraries`/`arguments` se añaden después de los de vanilla.
/// `assetIndex`, `downloads` y `javaVersion` se quedan tal cual (vienen de vanilla).
pub fn merge_with_fabric(mut vanilla: VersionMeta, fabric: FabricProfile) -> VersionMeta {
    vanilla.id = fabric.id;
    vanilla.main_class = fabric.main_class;
    vanilla.libraries.extend(fabric.libraries);

    let mut args = vanilla.arguments.take().unwrap_or_default();
    args.game.extend(fabric.arguments.game);
    args.jvm.extend(fabric.arguments.jvm);
    vanilla.arguments = Some(args);
    vanilla
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        instances::test_util::TempDir,
        minecraft::meta::fixtures::MODERN,
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    async fn route_server(routes: Vec<(&'static str, String)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
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
                        None => ("404 Not Found", String::new()),
                    };
                    let head = format!(
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(body.as_bytes()).await;
                });
            }
        });
        base
    }

    const LOADER_LIST: &str = r#"[
      {"loader":{"version":"0.16.0-beta.1","stable":false},"intermediary":{}},
      {"loader":{"version":"0.15.11","stable":true},"intermediary":{}},
      {"loader":{"version":"0.15.10","stable":true},"intermediary":{}}
    ]"#;

    const PROFILE: &str = r#"{
      "id": "fabric-loader-0.15.11-1.21.1",
      "inheritsFrom": "1.21.1",
      "releaseTime": "2024-08-08T12:00:00+00:00",
      "type": "release",
      "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
      "arguments": {"game": [], "jvm": ["-DFabricMcEmu=net.minecraft.client.main.Main"]},
      "libraries": [
        {"name": "net.fabricmc:fabric-loader:0.15.11", "url": "https://maven.fabricmc.net/"},
        {"name": "net.fabricmc:intermediary:1.21.1", "url": "https://maven.fabricmc.net/"}
      ]
    }"#;

    #[tokio::test]
    async fn list_loader_versions_and_pick_stable() -> Result<()> {
        let base = route_server(vec![("/versions/loader/1.21.1", LOADER_LIST.into())]).await;
        let dl = Downloader::insecure_for_tests()?;
        let versions = list_loader_versions(&dl, &base, "1.21.1").await?;
        assert_eq!(versions.len(), 3);
        assert_eq!(pick_stable(&versions)?, "0.15.11"); // la más reciente *estable*, no la beta
        assert!(pick_stable(&[]).is_err());
        let only_beta = [LoaderVersion {
            version: "0.16.0-beta.1".into(),
            stable: false,
        }];
        assert_eq!(pick_stable(&only_beta)?, "0.16.0-beta.1"); // sin estables: usa la primera
        Ok(())
    }

    #[tokio::test]
    async fn fetch_profile_downloads_then_reuses_disk_cache_without_network() -> Result<()> {
        let tmp = TempDir::new();
        let paths = NebulaPaths::new(tmp.path().join("c"), tmp.path().join("d"));
        let base = route_server(vec![(
            "/versions/loader/1.21.1/0.15.11/profile/json",
            PROFILE.into(),
        )])
        .await;
        let dl = Downloader::insecure_for_tests()?;

        let profile = fetch_profile(&dl, &base, &paths, "1.21.1", "0.15.11").await?;
        assert_eq!(profile.id, "fabric-loader-0.15.11-1.21.1");
        assert!(profile_cache_path(&paths, "1.21.1", "0.15.11")?.is_file());

        // Con la caché ya en disco, una base inalcanzable no debe impedir leerlo:
        // `insecure_for_tests` solo permite el host 127.0.0.1, así que si
        // `fetch_profile` intentara tocar la red aquí, fallaría con "host no
        // permitido" en vez de devolver el perfil.
        let cached = fetch_profile(&dl, "https://example.invalid", &paths, "1.21.1", "0.15.11").await?;
        assert_eq!(cached.id, profile.id);
        Ok(())
    }

    #[test]
    fn merge_appends_libraries_and_arguments_after_vanilla_and_overrides_main_class() {
        let vanilla: VersionMeta = serde_json::from_str(MODERN).expect("vanilla fixture");
        let vanilla_lib_count = vanilla.libraries.len();
        let vanilla_jvm_count = vanilla.arguments.as_ref().map_or(0, |a| a.jvm.len());
        let fabric: FabricProfile = serde_json::from_str(PROFILE).expect("fabric fixture");
        let fabric_lib_count = fabric.libraries.len();

        let merged = merge_with_fabric(vanilla.clone(), fabric);

        assert_eq!(merged.id, "fabric-loader-0.15.11-1.21.1");
        assert_eq!(
            merged.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        // assetIndex/downloads/javaVersion vienen de vanilla, sin cambios.
        assert_eq!(merged.asset_index.id, vanilla.asset_index.id);
        assert_eq!(
            merged.java_version.map(|j| j.major_version),
            vanilla.java_version.map(|j| j.major_version)
        );
        // Las libraries de Fabric se añaden DESPUÉS de las de vanilla.
        assert_eq!(merged.libraries.len(), vanilla_lib_count + fabric_lib_count);
        assert_eq!(
            merged.libraries[vanilla_lib_count].name,
            "net.fabricmc:fabric-loader:0.15.11"
        );
        // Los argumentos JVM de Fabric se añaden después de los de vanilla.
        let jvm = merged.arguments.as_ref().expect("arguments").jvm.len();
        assert_eq!(jvm, vanilla_jvm_count + 1);
    }
}
