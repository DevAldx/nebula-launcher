//! Descargas HTTPS verificadas: lista blanca de hosts, SHA-1, tamaño y escritura atómica.

use std::{
    io,
    path::{Path, PathBuf},
    time::Duration,
};

use futures_util::StreamExt;
use reqwest::{redirect, Client, Url};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use tokio::io::AsyncWriteExt;

use crate::{error::CoreError, filesystem, Result};

/// Hosts oficiales de Mojang desde los que se permite descargar.
pub const MOJANG_HOSTS: &[&str] = &[
    "piston-meta.mojang.com",
    "piston-data.mojang.com",
    "launchermeta.mojang.com",
    "launcher.mojang.com",
    "libraries.minecraft.net",
    "resources.download.minecraft.net",
];

const ATTEMPTS: u32 = 3;

/// Tope cuando el manifiesto no declara tamaño (mods de instancias
/// compartidas, por ejemplo): evita que un servidor remoto llene el disco.
const MAX_UNKNOWN_SIZE_BYTES: u64 = 1024 * 1024 * 1024; // 1 GiB

#[derive(Debug, Clone)]
pub struct Task {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
    /// SHA-256 en hexadecimal (lo usa Adoptium para los JRE).
    pub sha256: Option<String>,
    pub size: Option<u64>,
}

#[derive(Clone)]
pub struct Downloader {
    client: Client,
    hosts: Vec<String>,
    https_only: bool,
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha1_file(path: &Path) -> io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

/// ¿El archivo existe y coincide con el tamaño/hash esperados?
/// Sin tamaño ni hash conocidos, basta con que exista y no esté vacío.
pub async fn is_valid(path: PathBuf, sha1: Option<String>, size: Option<u64>) -> bool {
    tokio::task::spawn_blocking(move || {
        let Ok(meta) = std::fs::metadata(&path) else {
            return false;
        };
        if !meta.is_file() || size.is_some_and(|s| s != meta.len()) {
            return false;
        }
        match &sha1 {
            Some(expected) => sha1_file(&path).is_ok_and(|got| got.eq_ignore_ascii_case(expected)),
            None => meta.len() > 0,
        }
    })
    .await
    .unwrap_or(false)
}

impl Downloader {
    pub fn new(allowed_hosts: &[&str]) -> Result<Self> {
        Self::build(allowed_hosts, true)
    }

    fn build(allowed_hosts: &[&str], https_only: bool) -> Result<Self> {
        let hosts: Vec<String> = allowed_hosts.iter().map(|h| (*h).to_owned()).collect();
        let policy_hosts = hosts.clone();
        let client = Client::builder()
            .user_agent(concat!("NebulaLauncher/", env!("CARGO_PKG_VERSION")))
            .https_only(https_only)
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(30))
            // Un redirect nunca puede sacarnos de la lista blanca.
            .redirect(redirect::Policy::custom(move |attempt| {
                let ok = attempt.previous().len() < 5
                    && attempt
                        .url()
                        .host_str()
                        .is_some_and(|h| policy_hosts.iter().any(|a| a == h));
                if ok {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()
            .map_err(|e| CoreError::Network(e.to_string()))?;
        Ok(Self {
            client,
            hosts,
            https_only,
        })
    }

    #[cfg(test)]
    pub(crate) fn insecure_for_tests() -> Result<Self> {
        Self::build(&["127.0.0.1"], false)
    }

    fn check_url(&self, url: &str) -> Result<Url> {
        let parsed =
            Url::parse(url).map_err(|e| CoreError::Network(format!("URL inválida: {e}")))?;
        if self.https_only && parsed.scheme() != "https" {
            return Err(CoreError::Network(format!("solo HTTPS permitido: {url}")));
        }
        let host = parsed.host_str().unwrap_or_default();
        if !self.hosts.iter().any(|h| h == host) {
            return Err(CoreError::Network(format!("host no permitido: {host}")));
        }
        Ok(parsed)
    }

    /// Descarga en memoria (JSON de metadatos), con tope de tamaño.
    pub async fn fetch_bytes(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>> {
        let url = self.check_url(url)?;
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| CoreError::Network(e.to_string()))?;
        let mut out = Vec::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| CoreError::Network(e.to_string()))?;
            if out.len() + chunk.len() > max_bytes {
                return Err(CoreError::Network("respuesta demasiado grande".into()));
            }
            out.extend_from_slice(&chunk);
        }
        Ok(out)
    }

    /// Descarga `task` a `task.dest` (vía `.part` + rename) verificando tamaño y SHA-1.
    /// `on_bytes` recibe deltas; si un intento falla, recibe el delta negativo para
    /// que el progreso global no cuente bytes descartados.
    pub async fn download(
        &self,
        task: &Task,
        on_bytes: &(dyn Fn(i64) + Send + Sync),
    ) -> Result<()> {
        let mut last_err = CoreError::Network("sin intentos".into());
        for attempt in 0..ATTEMPTS {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(400 * u64::from(attempt))).await;
            }
            let mut counted: i64 = 0;
            match self.try_once(task, on_bytes, &mut counted).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    on_bytes(-counted);
                    last_err = e;
                }
            }
        }
        Err(last_err)
    }

    async fn try_once(
        &self,
        task: &Task,
        on_bytes: &(dyn Fn(i64) + Send + Sync),
        counted: &mut i64,
    ) -> Result<()> {
        let url = self.check_url(&task.url)?;
        if let Some(parent) = task.dest.parent() {
            filesystem::create_dir_all(parent)?;
        }
        // Nombre temporal único por intento: `create_new` no sigue symlinks y
        // dos descargas concurrentes al mismo destino no comparten el `.part`.
        let mut part_name = task.dest.as_os_str().to_os_string();
        part_name.push(format!(".{}.part", rand::random::<u64>()));
        let part = PathBuf::from(part_name);

        let resp = self
            .client
            .get(url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| CoreError::Network(format!("{}: {e}", task.url)))?;

        let result = async {
            let mut file = tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&part)
                .await
                .map_err(|e| CoreError::io(&part, e))?;
            let mut hasher = Sha1::new();
            let mut hasher256 = Sha256::new();
            let mut total: u64 = 0;
            let mut stream = resp.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| CoreError::Network(e.to_string()))?;
                total += chunk.len() as u64;
                let limit = task.size.unwrap_or(MAX_UNKNOWN_SIZE_BYTES);
                if total > limit {
                    return Err(CoreError::Integrity(format!(
                        "{} excede el tamaño máximo permitido",
                        task.url
                    )));
                }
                hasher.update(&chunk);
                hasher256.update(&chunk);
                file.write_all(&chunk)
                    .await
                    .map_err(|e| CoreError::io(&part, e))?;
                *counted += chunk.len() as i64;
                on_bytes(chunk.len() as i64);
            }
            file.flush().await.map_err(|e| CoreError::io(&part, e))?;
            if task.size.is_some_and(|s| s != total) {
                return Err(CoreError::Integrity(format!(
                    "{}: tamaño incorrecto",
                    task.url
                )));
            }
            if let Some(expected) = &task.sha1 {
                if !hex(&hasher.finalize()).eq_ignore_ascii_case(expected) {
                    return Err(CoreError::Integrity(format!(
                        "{}: SHA-1 no coincide",
                        task.url
                    )));
                }
            }
            if let Some(expected) = &task.sha256 {
                if !hex(&hasher256.finalize()).eq_ignore_ascii_case(expected) {
                    return Err(CoreError::Integrity(format!(
                        "{}: SHA-256 no coincide",
                        task.url
                    )));
                }
            }
            Ok(())
        }
        .await;

        match result {
            Ok(()) => {
                // En Windows `rename` no reemplaza un archivo existente; si el
                // destino estaba corrupto, hay que quitarlo antes.
                #[cfg(windows)]
                let _ = tokio::fs::remove_file(&task.dest).await;
                match tokio::fs::rename(&part, &task.dest).await {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        let _ = tokio::fs::remove_file(&part).await;
                        Err(CoreError::io(&task.dest, e))
                    }
                }
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&part).await;
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::test_util::TempDir;
    use std::sync::atomic::{AtomicI64, Ordering};
    use tokio::{io::AsyncReadExt, net::TcpListener};

    const BODY: &[u8] = b"hola nebula";

    /// Servidor HTTP mínimo que responde siempre `BODY`.
    async fn serve() -> Result<String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| CoreError::Network(e.to_string()))?;
        let addr = listener
            .local_addr()
            .map_err(|e| CoreError::Network(e.to_string()))?;
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = sock.read(&mut buf).await;
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        BODY.len()
                    );
                    let _ = sock.write_all(head.as_bytes()).await;
                    let _ = sock.write_all(BODY).await;
                });
            }
        });
        Ok(format!("http://{addr}/file"))
    }

    fn body_sha1() -> String {
        let mut h = Sha1::new();
        h.update(BODY);
        hex(&h.finalize())
    }

    #[tokio::test]
    async fn downloads_and_verifies() -> Result<()> {
        let tmp = TempDir::new();
        let dl = Downloader::insecure_for_tests()?;
        let task = Task {
            url: serve().await?,
            dest: tmp.path().join("a/b.bin"),
            sha1: Some(body_sha1()),
            sha256: None,
            size: Some(BODY.len() as u64),
        };
        let seen = AtomicI64::new(0);
        let cb = |d: i64| {
            seen.fetch_add(d, Ordering::Relaxed);
        };
        dl.download(&task, &cb).await?;
        assert_eq!(std::fs::read(&task.dest).unwrap_or_default(), BODY);
        assert_eq!(seen.load(Ordering::Relaxed), BODY.len() as i64);
        assert!(is_valid(task.dest.clone(), task.sha1.clone(), task.size).await);
        assert!(!tmp.path().join("a/b.bin.part").exists());
        Ok(())
    }

    #[tokio::test]
    async fn wrong_hash_fails_and_leaves_no_file() -> Result<()> {
        let tmp = TempDir::new();
        let dl = Downloader::insecure_for_tests()?;
        let task = Task {
            url: serve().await?,
            dest: tmp.path().join("x.bin"),
            sha1: Some("0".repeat(40)),
            sha256: None,
            size: None,
        };
        let seen = AtomicI64::new(0);
        let cb = |d: i64| {
            seen.fetch_add(d, Ordering::Relaxed);
        };
        assert!(matches!(
            dl.download(&task, &cb).await,
            Err(CoreError::Integrity(_))
        ));
        assert!(!task.dest.exists());
        assert_eq!(seen.load(Ordering::Relaxed), 0, "el progreso debe revertirse");
        Ok(())
    }

    #[tokio::test]
    async fn rejects_foreign_hosts_and_plain_http() -> Result<()> {
        let dl = Downloader::new(MOJANG_HOSTS)?;
        assert!(dl.check_url("https://evil.example.com/x").is_err());
        assert!(dl.check_url("http://piston-data.mojang.com/x").is_err());
        assert!(dl.check_url("https://piston-data.mojang.com/x").is_ok());
        assert!(dl
            .fetch_bytes("https://evil.example.com/x", 10)
            .await
            .is_err());
        Ok(())
    }

    #[tokio::test]
    async fn is_valid_detects_corruption() {
        let tmp = TempDir::new();
        let p = tmp.path().join("f");
        std::fs::write(&p, BODY).unwrap_or_default();
        assert!(is_valid(p.clone(), Some(body_sha1()), Some(BODY.len() as u64)).await);
        std::fs::write(&p, b"corrupto").unwrap_or_default();
        assert!(!is_valid(p, Some(body_sha1()), None).await);
    }
}
