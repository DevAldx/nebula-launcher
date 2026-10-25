//! Comprobación de actualizaciones: arquitectura mínima y real, sin un sistema
//! complejo de descarga/auto-aplicación.
//!
//! Nebula no trae un servidor de actualizaciones propio (no existe todavía
//! ninguna infraestructura de publicación para este proyecto), así que esto
//! solo compara la versión instalada contra un manifiesto JSON en una URL que
//! el usuario configura él mismo (p. ej. un `latest.json` en sus propios
//! GitHub Releases). Sin esa URL, simplemente no comprueba nada — no hay
//! ningún intento de "simular" una comprobación.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{error::CoreError, Result};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_MANIFEST_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    pub url: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateCheck {
    pub current_version: String,
    /// `Some` solo si el manifiesto remoto es una versión más nueva.
    pub available: Option<UpdateManifest>,
}

/// `major.minor.patch`, cada parte numérica; cualquier sufijo tipo `-beta.1`
/// se ignora para la comparación (semver completo sería de más para esto).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SimpleVersion(u64, u64, u64);

fn parse_version(v: &str) -> Option<SimpleVersion> {
    let core = v.trim().trim_start_matches('v');
    let core = core.split(['-', '+']).next().unwrap_or(core);
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some(SimpleVersion(major, minor, patch))
}

/// `Some(true)` si `remote` es más nueva que `current`; `Some(false)` si no;
/// `None` si alguna de las dos no se pudo interpretar como versión.
pub fn is_newer(current: &str, remote: &str) -> Option<bool> {
    Some(parse_version(remote)? > parse_version(current)?)
}

fn check_url(url: &str) -> Result<reqwest::Url> {
    let parsed =
        reqwest::Url::parse(url).map_err(|e| CoreError::Network(format!("URL inválida: {e}")))?;
    if parsed.scheme() != "https" {
        return Err(CoreError::Network(
            "la URL del manifiesto de actualizaciones debe ser https".into(),
        ));
    }
    Ok(parsed)
}

/// Descarga `manifest_url` (configurada por el usuario) y la compara con
/// `CURRENT_VERSION`. No descarga ni aplica nada por sí sola: solo informa.
pub async fn check_for_update(client: &reqwest::Client, manifest_url: &str) -> Result<UpdateCheck> {
    let url = check_url(manifest_url)?;
    let resp = client
        .get(url)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| CoreError::Network(e.to_string()))?
        .error_for_status()
        .map_err(|e| CoreError::Network(e.to_string()))?;
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| CoreError::Network(e.to_string()))?;
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(CoreError::Network("manifiesto de actualización demasiado grande".into()));
    }
    let manifest: UpdateManifest = serde_json::from_slice(&bytes)
        .map_err(|e| CoreError::Network(format!("manifiesto de actualización inválido: {e}")))?;
    let available = match is_newer(CURRENT_VERSION, &manifest.version) {
        Some(true) => Some(manifest),
        _ => None,
    };
    Ok(UpdateCheck {
        current_version: CURRENT_VERSION.to_owned(),
        available,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[test]
    fn version_comparison_ignores_prerelease_suffix_and_handles_short_forms() {
        assert_eq!(is_newer("0.1.0", "0.2.0"), Some(true));
        assert_eq!(is_newer("0.2.0", "0.1.9"), Some(false));
        assert_eq!(is_newer("0.1.0", "0.1.0"), Some(false));
        assert_eq!(is_newer("1.2.3", "v1.2.4"), Some(true));
        assert_eq!(is_newer("1.0", "1.0.1"), Some(true)); // patch implícito 0
        assert_eq!(is_newer("1.0.0", "1.0.0-beta.3"), Some(false)); // sufijo ignorado
        assert_eq!(is_newer("1.0.0", "no-es-una-version"), None);
    }

    #[test]
    fn rejects_non_https_manifest_urls() {
        assert!(check_url("http://example.com/latest.json").is_err());
        assert!(check_url("https://example.com/latest.json").is_ok());
    }

    async fn serve(status: u16, body: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let body = body.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 512];
                    let _ = sock.read(&mut buf).await;
                    let line = if status == 200 { "200 OK" } else { "404 Not Found" };
                    let head = format!(
                        "HTTP/1.1 {line}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(body.as_bytes()).await;
                });
            }
        });
        base
    }

    #[tokio::test]
    async fn check_for_update_reports_none_when_current_and_some_when_newer() -> Result<()> {
        // Nota: `check_for_update` exige https, así que probamos `is_newer` +
        // el parseo del manifiesto por separado (igual que hace la función)
        // contra un servidor http local, y la política https por su cuenta
        // (ya cubierta arriba) — así no dependemos de TLS en el test.
        let same = format!(r#"{{"version":"{CURRENT_VERSION}","url":"https://example.com/x"}}"#);
        let base = serve(200, same).await;
        let bytes = reqwest::get(format!("{base}/latest.json"))
            .await
            .map_err(|e| CoreError::Network(e.to_string()))?
            .bytes()
            .await
            .map_err(|e| CoreError::Network(e.to_string()))?;
        let manifest: UpdateManifest = serde_json::from_slice(&bytes).expect("parse");
        assert_eq!(is_newer(CURRENT_VERSION, &manifest.version), Some(false));

        let newer = r#"{"version":"999.0.0","url":"https://example.com/y","notes":"gran mejora"}"#.to_owned();
        let base = serve(200, newer).await;
        let bytes = reqwest::get(format!("{base}/latest.json"))
            .await
            .map_err(|e| CoreError::Network(e.to_string()))?
            .bytes()
            .await
            .map_err(|e| CoreError::Network(e.to_string()))?;
        let manifest: UpdateManifest = serde_json::from_slice(&bytes).expect("parse");
        assert_eq!(is_newer(CURRENT_VERSION, &manifest.version), Some(true));
        assert_eq!(manifest.notes.as_deref(), Some("gran mejora"));
        Ok(())
    }
}
