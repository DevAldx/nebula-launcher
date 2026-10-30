//! Flujo de código de dispositivo de Microsoft (OAuth2 Device Authorization Grant).
//!
//! No requiere URI de redirección ni abrir un navegador embebido: el usuario visita
//! una página de Microsoft en su propio navegador e introduce un código.

use std::time::{Duration, Instant};

use serde::Deserialize;

use super::endpoints::AuthEndpoints;
use crate::{error::CoreError, Result};

/// `XboxLive.signin` es obligatorio (si no, el paso de Xbox Live falla).
/// `offline_access` pide un refresh token para no repetir el login en cada sesión.
const SCOPE: &str = "XboxLive.signin offline_access";

#[derive(Debug, Clone)]
pub struct DeviceCode {
    pub(super) device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval: Duration,
    pub expires_at: Instant,
}

#[derive(Debug, Clone)]
pub struct MsaTokens {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Deserialize)]
struct DeviceCodeResp {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Deserialize, Default)]
struct TokenResp {
    access_token: Option<String>,
    refresh_token: Option<String>,
    error: Option<String>,
}

pub enum PollStep {
    Pending,
    SlowDown,
    Success(MsaTokens),
}

pub async fn request_device_code(
    client: &reqwest::Client,
    ep: &AuthEndpoints,
    client_id: &str,
) -> Result<DeviceCode> {
    let url = ep.check(&ep.device_code_url)?;
    let resp = client
        .post(url)
        .form(&[("client_id", client_id), ("scope", SCOPE)])
        .send()
        .await
        .map_err(|e| CoreError::Network(e.to_string()))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| CoreError::Network(e.to_string()))?;
    if !status.is_success() {
        return Err(describe_oauth_error(status.as_u16(), &text, false));
    }
    let parsed: DeviceCodeResp = serde_json::from_str(&text)
        .map_err(|e| CoreError::Auth(format!("respuesta de Microsoft inesperada: {e}")))?;
    Ok(DeviceCode {
        device_code: parsed.device_code,
        user_code: parsed.user_code,
        verification_uri: parsed.verification_uri,
        interval: Duration::from_secs(parsed.interval),
        expires_at: Instant::now() + Duration::from_secs(parsed.expires_in),
    })
}

pub async fn poll_once(
    client: &reqwest::Client,
    ep: &AuthEndpoints,
    client_id: &str,
    device_code: &str,
) -> Result<PollStep> {
    let url = ep.check(&ep.token_url)?;
    let resp = client
        .post(url)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", client_id),
            ("device_code", device_code),
        ])
        .send()
        .await
        .map_err(|e| CoreError::Network(e.to_string()))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| CoreError::Network(e.to_string()))?;
    if status.is_success() {
        let parsed: TokenResp = serde_json::from_str(&text)
            .map_err(|e| CoreError::Auth(format!("respuesta de Microsoft inesperada: {e}")))?;
        return match (parsed.access_token, parsed.refresh_token) {
            (Some(access_token), Some(refresh_token)) => Ok(PollStep::Success(MsaTokens {
                access_token,
                refresh_token,
            })),
            _ => Err(CoreError::Auth(
                "Microsoft no devolvió un token de acceso válido".into(),
            )),
        };
    }
    let parsed: TokenResp = serde_json::from_str(&text).unwrap_or_default();
    match parsed.error.as_deref() {
        Some("authorization_pending") => Ok(PollStep::Pending),
        Some("slow_down") => Ok(PollStep::SlowDown),
        Some("expired_token") => Err(CoreError::Auth(
            "el código expiró antes de completar el inicio de sesión; vuelve a intentarlo."
                .into(),
        )),
        Some("authorization_declined") => Err(CoreError::Auth(
            "se canceló el inicio de sesión en el navegador.".into(),
        )),
        _ => Err(describe_oauth_error(status.as_u16(), &text, false)),
    }
}

pub async fn refresh(
    client: &reqwest::Client,
    ep: &AuthEndpoints,
    client_id: &str,
    refresh_token: &str,
) -> Result<MsaTokens> {
    let url = ep.check(&ep.token_url)?;
    let resp = client
        .post(url)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh_token),
            ("scope", SCOPE),
        ])
        .send()
        .await
        .map_err(|e| CoreError::Network(e.to_string()))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| CoreError::Network(e.to_string()))?;
    if !status.is_success() {
        return Err(describe_oauth_error(status.as_u16(), &text, true));
    }
    let parsed: TokenResp = serde_json::from_str(&text)
        .map_err(|e| CoreError::Auth(format!("respuesta de Microsoft inesperada: {e}")))?;
    match (parsed.access_token, parsed.refresh_token) {
        (Some(access_token), Some(refresh_token)) => Ok(MsaTokens {
            access_token,
            refresh_token,
        }),
        _ => Err(CoreError::Auth(
            "no se pudo renovar la sesión; inicia sesión de nuevo.".into(),
        )),
    }
}

fn describe_oauth_error(status: u16, body: &str, session_expired: bool) -> CoreError {
    // Un refresh token revocado es definitivo; un timeout o un 5xx no.
    if let Ok(parsed) = serde_json::from_str::<TokenResp>(body) {
        if let Some(error) = parsed.error.as_deref() {
            if session_expired
                && matches!(
                    error,
                    "invalid_grant" | "invalid_token" | "interaction_required" | "consent_required"
                )
            {
                return CoreError::AuthExpired(format!(
                    "Microsoft rechazó la sesión guardada ({error})"
                ));
            }
        }
    }
    if body.contains("AADSTS700016") {
        return CoreError::Auth(
            "Microsoft no reconoce ese Client ID. Revisa Configuración > Cuenta: debe ser el \
             GUID de tu propia aplicación de Azure, registrada para cuentas personales."
                .into(),
        );
    }
    if body.contains("AADSTS70016") || body.contains("AADSTS50059") {
        return CoreError::Auth(
            "la aplicación de Azure no tiene habilitado el flujo de código de dispositivo \
             («Allow public client flows» debe estar en Sí)."
                .into(),
        );
    }
    CoreError::Auth(format!("error de Microsoft ({status}): {body}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    /// Servidor que responde `body` con `status` a cualquier petición a `path`,
    /// y 404 a cualquier otra ruta. Si `sequence` tiene más de un elemento, cada
    /// petición a `path` consume el siguiente par (status, body) de la lista.
    async fn route_server(path: &'static str, sequence: Vec<(u16, &'static str)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        let idx = Arc::new(AtomicUsize::new(0));
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let idx = idx.clone();
                let sequence = sequence.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let req_path = req.split_whitespace().nth(1).unwrap_or("/");
                    let (status, body) = if req_path == path {
                        let i = idx.fetch_add(1, Ordering::SeqCst).min(sequence.len() - 1);
                        sequence[i]
                    } else {
                        (404, "")
                    };
                    let line = match status {
                        200 => "200 OK",
                        400 => "400 Bad Request",
                        _ => "500 Internal Server Error",
                    };
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
    async fn request_device_code_parses_success() -> Result<()> {
        let body = r#"{"device_code":"dc1","user_code":"ABCD-EFGH","verification_uri":"https://www.microsoft.com/link","expires_in":900,"interval":5}"#;
        let base = route_server("/devicecode", vec![(200, body)]).await;
        let ep = AuthEndpoints::for_tests(&base);
        let client = reqwest::Client::new();
        let code = request_device_code(&client, &ep, "test-client").await?;
        assert_eq!(code.user_code, "ABCD-EFGH");
        assert_eq!(code.verification_uri, "https://www.microsoft.com/link");
        assert_eq!(code.interval, Duration::from_secs(5));
        Ok(())
    }

    #[tokio::test]
    async fn poll_once_covers_all_outcomes() -> Result<()> {
        let pending = route_server(
            "/token",
            vec![(400, r#"{"error":"authorization_pending"}"#)],
        )
        .await;
        let ep = AuthEndpoints::for_tests(&pending);
        let client = reqwest::Client::new();
        assert!(matches!(
            poll_once(&client, &ep, "c", "dc").await?,
            PollStep::Pending
        ));

        let slow = route_server("/token", vec![(400, r#"{"error":"slow_down"}"#)]).await;
        let ep = AuthEndpoints::for_tests(&slow);
        assert!(matches!(
            poll_once(&client, &ep, "c", "dc").await?,
            PollStep::SlowDown
        ));

        let ok = route_server(
            "/token",
            vec![(200, r#"{"access_token":"at","refresh_token":"rt"}"#)],
        )
        .await;
        let ep = AuthEndpoints::for_tests(&ok);
        match poll_once(&client, &ep, "c", "dc").await? {
            PollStep::Success(t) => {
                assert_eq!(t.access_token, "at");
                assert_eq!(t.refresh_token, "rt");
            }
            _ => panic!("se esperaba Success"),
        }

        let expired = route_server("/token", vec![(400, r#"{"error":"expired_token"}"#)]).await;
        let ep = AuthEndpoints::for_tests(&expired);
        assert!(poll_once(&client, &ep, "c", "dc").await.is_err());

        let declined =
            route_server("/token", vec![(400, r#"{"error":"authorization_declined"}"#)]).await;
        let ep = AuthEndpoints::for_tests(&declined);
        assert!(poll_once(&client, &ep, "c", "dc").await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn invalid_client_id_gets_a_clear_message() -> Result<()> {
        let base = route_server(
            "/devicecode",
            vec![(
                400,
                r#"{"error":"unauthorized_client","error_description":"AADSTS700016: Application with identifier 'bad' was not found"}"#,
            )],
        )
        .await;
        let ep = AuthEndpoints::for_tests(&base);
        let client = reqwest::Client::new();
        let err = request_device_code(&client, &ep, "bad")
            .await
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(err.contains("Client ID"), "{err}");
        Ok(())
    }

    #[tokio::test]
    async fn refresh_round_trips_tokens() -> Result<()> {
        let base = route_server(
            "/token",
            vec![(200, r#"{"access_token":"at2","refresh_token":"rt2"}"#)],
        )
        .await;
        let ep = AuthEndpoints::for_tests(&base);
        let client = reqwest::Client::new();
        let tokens = refresh(&client, &ep, "c", "old-rt").await?;
        assert_eq!(tokens.access_token, "at2");
        assert_eq!(tokens.refresh_token, "rt2");
        Ok(())
    }

    #[test]
    fn rejects_non_allowed_host_even_for_live_endpoints() {
        let ep = AuthEndpoints::live();
        assert!(ep.check("https://evil.example.com/token").is_err());
        assert!(ep
            .check("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
            .is_ok());
        assert!(ep
            .check("http://login.microsoftonline.com/consumers/oauth2/v2.0/token")
            .is_err());
    }

    #[test]
    fn only_refresh_treats_invalid_grant_as_expired_session() {
        let body = r#"{"error":"invalid_grant"}"#;
        assert!(matches!(
            describe_oauth_error(400, body, true),
            CoreError::AuthExpired(_)
        ));
        assert!(matches!(
            describe_oauth_error(400, body, false),
            CoreError::Auth(_)
        ));
        // Un 5xx nunca debe borrar la sesión guardada.
        assert!(matches!(
            describe_oauth_error(503, "<html>caído</html>", true),
            CoreError::Auth(_)
        ));
    }
}
