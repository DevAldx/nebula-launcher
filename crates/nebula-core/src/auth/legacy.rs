//! Inicio de sesión heredado de `login.live.com`.
//!
//! Es el flujo que usaba el launcher oficial de Minecraft antes de Azure. Utiliza
//! el identificador público del launcher oficial (`00000000402b5328`), así que
//! **no requiere una app de Azure propia**. A cambio, no es una integración
//! oficial: Microsoft puede cambiarlo o bloquearlo sin aviso.
//!
//! Usa el flujo de código de dispositivo de Live Connect: el usuario visita una
//! página de Microsoft en su navegador e introduce un código. Nunca se pide ni se
//! ve la contraseña de Microsoft dentro de Nebula.
//!
//! El token de Live Connect se encadena con Xbox Live usando el prefijo `t=`
//! (los tokens de Azure usan `d=`).

use std::time::{Duration, Instant};

use serde::Deserialize;

use super::{
    device_flow::{DeviceCode, MsaTokens, PollStep},
    endpoints::AuthEndpoints,
};
use crate::{error::CoreError, Result};

/// Client ID público del launcher oficial de Minecraft. No es un secreto.
pub const LEGACY_CLIENT_ID: &str = "00000000402b5328";
/// `MBI_SSL` autoriza la cuenta para Xbox Live. El endpoint heredado también
/// devuelve un refresh token con este scope (no acepta `wl.offline_access`).
const LEGACY_SCOPE: &str = "service::user.auth.xboxlive.com::MBI_SSL";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

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
    error_description: Option<String>,
}

/// Pide un código de dispositivo al endpoint heredado de Live Connect.
pub async fn request_device_code(
    client: &reqwest::Client,
    ep: &AuthEndpoints,
) -> Result<DeviceCode> {
    let url = ep.check(&ep.legacy_device_code_url)?;
    let resp = client
        .post(url)
        .form(&[
            ("client_id", LEGACY_CLIENT_ID),
            ("scope", LEGACY_SCOPE),
            ("response_type", "device_code"),
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
        return Err(describe_oauth_error(status.as_u16(), &text));
    }
    let parsed: DeviceCodeResp = serde_json::from_str(&text).map_err(|e| {
        CoreError::Auth(format!(
            "respuesta inesperada del inicio de sesión heredado: {e}"
        ))
    })?;
    Ok(DeviceCode {
        device_code: parsed.device_code,
        user_code: parsed.user_code,
        verification_uri: parsed.verification_uri,
        interval: Duration::from_secs(parsed.interval.max(1)),
        expires_at: Instant::now() + Duration::from_secs(parsed.expires_in),
    })
}

/// Un sondeo del endpoint de tokens heredado.
pub async fn poll_once(
    client: &reqwest::Client,
    ep: &AuthEndpoints,
    device_code: &str,
) -> Result<PollStep> {
    let url = ep.check(&ep.legacy_token_url)?;
    let resp = client
        .post(url)
        .form(&[
            ("grant_type", DEVICE_GRANT),
            ("client_id", LEGACY_CLIENT_ID),
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
        let parsed: TokenResp = serde_json::from_str(&text).map_err(|e| {
            CoreError::Auth(format!(
                "respuesta inesperada del inicio de sesión heredado: {e}"
            ))
        })?;
        return match (parsed.access_token, parsed.refresh_token) {
            (Some(access_token), Some(refresh_token)) => Ok(PollStep::Success(MsaTokens {
                access_token,
                refresh_token,
            })),
            _ => Err(CoreError::Auth(
                "Microsoft no devolvió un token de acceso válido.".into(),
            )),
        };
    }

    let parsed: TokenResp = serde_json::from_str(&text).unwrap_or_default();
    match parsed.error.as_deref() {
        Some("authorization_pending") => Ok(PollStep::Pending),
        Some("slow_down") => Ok(PollStep::SlowDown),
        Some("expired_token") | Some("bad_verification_code") => Err(CoreError::Auth(
            "el código expiró antes de completar el inicio de sesión; vuelve a intentarlo."
                .into(),
        )),
        Some("authorization_declined") => Err(CoreError::Auth(
            "se canceló el inicio de sesión en el navegador.".into(),
        )),
        _ => Err(describe_oauth_error(status.as_u16(), &text)),
    }
}

/// Renueva los tokens con el endpoint heredado, sin pedir el Client ID de Azure.
pub async fn refresh(
    client: &reqwest::Client,
    ep: &AuthEndpoints,
    refresh_token: &str,
) -> Result<MsaTokens> {
    let url = ep.check(&ep.legacy_token_url)?;
    let resp = client
        .post(url)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", LEGACY_CLIENT_ID),
            ("refresh_token", refresh_token),
            ("scope", LEGACY_SCOPE),
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
        return Err(describe_oauth_error(status.as_u16(), &text));
    }
    let parsed: TokenResp = serde_json::from_str(&text).map_err(|e| {
        CoreError::Auth(format!(
            "respuesta inesperada del inicio de sesión heredado: {e}"
        ))
    })?;
    match (parsed.access_token, parsed.refresh_token) {
        (Some(access_token), Some(refresh_token)) => Ok(MsaTokens {
            access_token,
            refresh_token,
        }),
        _ => Err(CoreError::Auth(
            "no se pudo renovar la sesión heredada; inicia sesión de nuevo.".into(),
        )),
    }
}

fn describe_oauth_error(status: u16, body: &str) -> CoreError {
    let parsed: TokenResp = serde_json::from_str(body).unwrap_or_default();
    if let Some(error) = parsed.error {
        let description = parsed
            .error_description
            .map(|d| format!(": {d}"))
            .unwrap_or_default();
        return CoreError::Auth(format!(
            "Microsoft rechazó el inicio de sesión heredado ({error}){description}"
        ));
    }
    CoreError::Auth(format!(
        "error del inicio de sesión heredado de Microsoft ({status}): {body}"
    ))
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

    /// Servidor que responde al código de dispositivo y una secuencia de
    /// respuestas al endpoint de tokens.
    async fn legacy_server(
        device: &'static str,
        token_sequence: Vec<(u16, &'static str)>,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        let idx = Arc::new(AtomicUsize::new(0));
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let idx = idx.clone();
                let token_sequence = token_sequence.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let path = req.split_whitespace().nth(1).unwrap_or("/");
                    let (status, body) = if path == "/legacy_device_code" {
                        (200, device)
                    } else if path == "/legacy_token" && !token_sequence.is_empty() {
                        let i = idx.fetch_add(1, Ordering::SeqCst).min(token_sequence.len() - 1);
                        token_sequence[i]
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
    async fn requests_a_device_code_from_live_connect() -> Result<()> {
        let body = r#"{"device_code":"dc1","user_code":"ABCD-EFGH","verification_uri":"https://www.microsoft.com/link","expires_in":900,"interval":5}"#;
        let base = legacy_server(body, vec![]).await;
        let ep = AuthEndpoints::for_tests(&base);
        let client = reqwest::Client::new();
        let code = request_device_code(&client, &ep).await?;
        assert_eq!(code.user_code, "ABCD-EFGH");
        assert_eq!(code.verification_uri, "https://www.microsoft.com/link");
        assert_eq!(code.interval, Duration::from_secs(5));
        Ok(())
    }

    #[tokio::test]
    async fn poll_covers_pending_success_and_errors() -> Result<()> {
        let client = reqwest::Client::new();

        let pending = legacy_server(
            "{}",
            vec![(400, r#"{"error":"authorization_pending"}"#)],
        )
        .await;
        let ep = AuthEndpoints::for_tests(&pending);
        assert!(matches!(
            poll_once(&client, &ep, "dc").await?,
            PollStep::Pending
        ));

        let slow = legacy_server("{}", vec![(400, r#"{"error":"slow_down"}"#)]).await;
        let ep = AuthEndpoints::for_tests(&slow);
        assert!(matches!(
            poll_once(&client, &ep, "dc").await?,
            PollStep::SlowDown
        ));

        let ok = legacy_server(
            "{}",
            vec![(
                200,
                r#"{"access_token":"at","refresh_token":"rt","scope":"service::user.auth.xboxlive.com::MBI_SSL"}"#,
            )],
        )
        .await;
        let ep = AuthEndpoints::for_tests(&ok);
        match poll_once(&client, &ep, "dc").await? {
            PollStep::Success(tokens) => {
                assert_eq!(tokens.access_token, "at");
                assert_eq!(tokens.refresh_token, "rt");
            }
            _ => panic!("se esperaba Success"),
        }

        let expired = legacy_server("{}", vec![(400, r#"{"error":"expired_token"}"#)]).await;
        let ep = AuthEndpoints::for_tests(&expired);
        assert!(poll_once(&client, &ep, "dc").await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn refreshes_with_the_legacy_endpoint() -> Result<()> {
        let base = legacy_server(
            "{}",
            vec![(200, r#"{"access_token":"at2","refresh_token":"rt2"}"#)],
        )
        .await;
        let ep = AuthEndpoints::for_tests(&base);
        let client = reqwest::Client::new();
        let tokens = refresh(&client, &ep, "old-rt").await?;
        assert_eq!(tokens.access_token, "at2");
        assert_eq!(tokens.refresh_token, "rt2");
        Ok(())
    }
}
