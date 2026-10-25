//! Autenticación con Xbox Live (XBL) y autorización XSTS para Minecraft.

use serde::Deserialize;

use super::endpoints::AuthEndpoints;
use crate::{error::CoreError, Result};

/// Solo se necesita el token: el hash de usuario (`uhs`) que de verdad importa
/// para iniciar sesión en Minecraft es el que devuelve el paso de XSTS, no este.
#[derive(Debug, Clone)]
pub struct XblToken {
    pub token: String,
}

#[derive(Debug, Clone)]
pub struct XstsToken {
    pub token: String,
    pub uhs: String,
}

#[derive(Deserialize)]
struct XblResp {
    #[serde(rename = "Token")]
    token: String,
}

#[derive(Deserialize)]
struct XuiEntry {
    uhs: String,
}

#[derive(Deserialize)]
struct DisplayClaims {
    xui: Vec<XuiEntry>,
}

#[derive(Deserialize)]
struct XstsOkResp {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: DisplayClaims,
}

pub async fn authenticate_xbl(
    client: &reqwest::Client,
    ep: &AuthEndpoints,
    msa_access_token: &str,
    ticket_prefix: &str,
) -> Result<XblToken> {
    let url = ep.check(&ep.xbl_url)?;
    let body = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("{ticket_prefix}={msa_access_token}"),
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT",
    });
    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| CoreError::Network(e.to_string()))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| CoreError::Network(e.to_string()))?;
    if !status.is_success() {
        return Err(CoreError::Auth(format!(
            "Xbox Live rechazó el inicio de sesión ({status}): {text}"
        )));
    }
    let parsed: XblResp = serde_json::from_str(&text)
        .map_err(|e| CoreError::Auth(format!("respuesta de Xbox Live inesperada: {e}")))?;
    Ok(XblToken {
        token: parsed.token,
    })
}

#[derive(Deserialize)]
struct XstsErrResp {
    #[serde(rename = "XErr")]
    xerr: Option<i64>,
}

pub async fn authorize_xsts(
    client: &reqwest::Client,
    ep: &AuthEndpoints,
    xbl_token: &str,
) -> Result<XstsToken> {
    let url = ep.check(&ep.xsts_url)?;
    let body = serde_json::json!({
        "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT",
    });
    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| CoreError::Network(e.to_string()))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| CoreError::Network(e.to_string()))?;
    if status.is_success() {
        let parsed: XstsOkResp = serde_json::from_str(&text)
            .map_err(|e| CoreError::Auth(format!("respuesta de XSTS inesperada: {e}")))?;
        let uhs = parsed
            .display_claims
            .xui
            .into_iter()
            .next()
            .map(|x| x.uhs)
            .ok_or_else(|| CoreError::Auth("XSTS no devolvió el hash de usuario".into()))?;
        return Ok(XstsToken {
            token: parsed.token,
            uhs,
        });
    }
    let xerr = serde_json::from_str::<XstsErrResp>(&text)
        .ok()
        .and_then(|e| e.xerr);
    Err(CoreError::Auth(describe_xsts_error(xerr, &text)))
}

/// Códigos documentados de `xsts.auth.xboxlive.com`.
fn describe_xsts_error(xerr: Option<i64>, raw: &str) -> String {
    match xerr {
        Some(2148916227) => {
            "esta cuenta está suspendida de Xbox por infringir las normas de la comunidad."
                .into()
        }
        Some(2148916233) => "esta cuenta de Microsoft todavía no tiene un perfil de Xbox. \
             Créalo entrando una vez en https://www.xbox.com y vuelve a intentarlo."
            .into(),
        Some(2148916235) => "Xbox Live no está disponible en el país de esta cuenta.".into(),
        Some(2148916236) | Some(2148916237) => {
            "esta cuenta necesita verificación de edad en xbox.com antes de continuar.".into()
        }
        Some(2148916238) => "esta cuenta pertenece a un menor de 18 años; un adulto debe \
             añadirla a un grupo familiar en account.microsoft.com/family."
            .into(),
        Some(code) => format!("Xbox Live rechazó la cuenta (código {code})."),
        None => format!("Xbox Live rechazó la cuenta: {raw}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    async fn single_route(path: &'static str, status: u16, body: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let body = body.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let got_path = req.split_whitespace().nth(1).unwrap_or("/");
                    let (status, body) = if got_path == path {
                        (status, body)
                    } else {
                        (404, String::new())
                    };
                    let line = if status == 200 { "200 OK" } else { "401 Unauthorized" };
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
    async fn xbl_success_extracts_token() -> Result<()> {
        let body = r#"{"Token":"xbltok","DisplayClaims":{"xui":[{"uhs":"hash123"}]}}"#;
        let base = single_route("/xbl", 200, body.to_owned()).await;
        let ep = AuthEndpoints::for_tests(&base);
        let client = reqwest::Client::new();
        let xbl = authenticate_xbl(&client, &ep, "msa-token", "d").await?;
        assert_eq!(xbl.token, "xbltok");
        Ok(())
    }

    #[tokio::test]
    async fn xsts_success_extracts_token_and_uhs() -> Result<()> {
        let body = r#"{"Token":"xststok","DisplayClaims":{"xui":[{"uhs":"hash123"}]}}"#;
        let base = single_route("/xsts", 200, body.to_owned()).await;
        let ep = AuthEndpoints::for_tests(&base);
        let client = reqwest::Client::new();
        let xsts = authorize_xsts(&client, &ep, "xbl-token").await?;
        assert_eq!(xsts.token, "xststok");
        Ok(())
    }

    #[tokio::test]
    async fn xsts_maps_known_error_codes_to_clear_messages() -> Result<()> {
        let cases: &[(i64, &str)] = &[
            (2148916233, "perfil de Xbox"),
            (2148916235, "no está disponible en el país"),
            (2148916236, "verificación de edad"),
            (2148916238, "menor de 18"),
        ];
        let client = reqwest::Client::new();
        for (code, needle) in cases {
            let body = format!(r#"{{"Identity":"0","XErr":{code},"Message":""}}"#);
            let base = single_route("/xsts", 401, body).await;
            let ep = AuthEndpoints::for_tests(&base);
            let err = authorize_xsts(&client, &ep, "xbl-token")
                .await
                .err()
                .map(|e| e.to_string())
                .unwrap_or_default();
            assert!(err.contains(needle), "code {code}: {err}");
        }
        Ok(())
    }
}
