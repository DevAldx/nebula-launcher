//! Autenticación con Minecraft Services y obtención del perfil (UUID, nombre, skin).

use serde::{Deserialize, Serialize};

use super::endpoints::AuthEndpoints;
use crate::{error::CoreError, Result};

#[derive(Debug, Clone)]
pub struct McSession {
    pub access_token: String,
}

#[derive(Deserialize)]
struct McLoginResp {
    access_token: String,
}

pub async fn login_with_xbox(
    client: &reqwest::Client,
    ep: &AuthEndpoints,
    uhs: &str,
    xsts_token: &str,
) -> Result<McSession> {
    let url = ep.check(&ep.mc_login_url)?;
    let body = serde_json::json!({ "identityToken": format!("XBL3.0 x={uhs};{xsts_token}") });
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
        if status.as_u16() == 403 && text.contains("Invalid app registration") {
            return Err(CoreError::Auth(
                "Microsoft rechazó este Client ID para la API de Minecraft (\"Invalid app \
                 registration\"). Tu aplicación de Azure necesita aprobación: envíala en \
                 https://aka.ms/mce-reviewappid (ver el README para los pasos completos)."
                    .into(),
            ));
        }
        return Err(CoreError::Auth(format!(
            "Minecraft rechazó el inicio de sesión ({status}): {text}"
        )));
    }
    let parsed: McLoginResp = serde_json::from_str(&text)
        .map_err(|e| CoreError::Auth(format!("respuesta de Minecraft inesperada: {e}")))?;
    Ok(McSession {
        access_token: parsed.access_token,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    /// UUID con guiones (formato 8-4-4-4-12).
    pub uuid: String,
    pub username: String,
    pub skin_url: Option<String>,
}

#[derive(Deserialize)]
struct ProfileResp {
    id: String,
    name: String,
    #[serde(default)]
    skins: Vec<SkinEntry>,
}

#[derive(Deserialize)]
struct SkinEntry {
    state: String,
    url: String,
}

pub async fn fetch_profile(
    client: &reqwest::Client,
    ep: &AuthEndpoints,
    mc_access_token: &str,
) -> Result<Profile> {
    let url = ep.check(&ep.mc_profile_url)?;
    let resp = client
        .get(url)
        .bearer_auth(mc_access_token)
        .send()
        .await
        .map_err(|e| CoreError::Network(e.to_string()))?;
    let status = resp.status();
    if status.as_u16() == 404 {
        return Err(CoreError::Auth(
            "esta cuenta de Microsoft no tiene Minecraft: Java Edition. Si lo obtuviste con \
             Xbox Game Pass, entra una vez con el launcher oficial de Minecraft para crear tu \
             perfil y vuelve a intentarlo aquí."
                .into(),
        ));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| CoreError::Network(e.to_string()))?;
    if !status.is_success() {
        return Err(CoreError::Auth(format!(
            "no se pudo obtener el perfil de Minecraft ({status}): {text}"
        )));
    }
    let parsed: ProfileResp = serde_json::from_str(&text)
        .map_err(|e| CoreError::Auth(format!("respuesta de perfil inesperada: {e}")))?;
    let skin_url = parsed
        .skins
        .into_iter()
        .find(|s| s.state == "ACTIVE")
        .map(|s| normalize_skin_url(&s.url));
    let username = parsed.name;
    if username.is_empty()
        || username.len() > 16
        || !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(CoreError::Auth(
            "el perfil de Minecraft devolvió un nombre de usuario no válido".into(),
        ));
    }
    Ok(Profile {
        uuid: dash_uuid(&parsed.id)?,
        username,
        skin_url,
    })
}

/// La API de Minecraft sigue devolviendo la skin por `http://textures.minecraft.net`.
/// La CSP de Nebula solo permite HTTPS (y es lo correcto), pero el mismo recurso
/// responde por HTTPS, así que normalizamos el esquema antes de guardarlo.
pub fn normalize_skin_url(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("http://textures.minecraft.net/") {
        format!("https://textures.minecraft.net/{rest}")
    } else {
        url.to_owned()
    }
}

/// `api.minecraftservices.com` devuelve el UUID sin guiones; `--uuid` de Minecraft
/// espera el formato con guiones (8-4-4-4-12).
pub fn dash_uuid(trimmed: &str) -> Result<String> {
    let t = trimmed.trim();
    let valid = t.len() == 32 && t.bytes().all(|b| b.is_ascii_hexdigit());
    if !valid {
        return Err(CoreError::Auth(format!(
            "Minecraft devolvió un UUID con un formato inesperado: {trimmed}"
        )));
    }
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &t[0..8],
        &t[8..12],
        &t[12..16],
        &t[16..20],
        &t[20..32]
    ))
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
                    let line = match status {
                        200 => "200 OK",
                        403 => "403 Forbidden",
                        404 => "404 Not Found",
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

    #[test]
    fn skin_urls_are_upgraded_to_https() {
        assert_eq!(
            normalize_skin_url("http://textures.minecraft.net/texture/abc"),
            "https://textures.minecraft.net/texture/abc"
        );
        assert_eq!(
            normalize_skin_url("https://textures.minecraft.net/texture/abc"),
            "https://textures.minecraft.net/texture/abc"
        );
        assert_eq!(
            normalize_skin_url("https://example.com/skin.png"),
            "https://example.com/skin.png"
        );
    }

    #[test]
    fn dash_uuid_formats_and_validates() {
        assert_eq!(
            dash_uuid("069a79f444e94726a5befca90e38aaf5").unwrap_or_default(),
            "069a79f4-44e9-4726-a5be-fca90e38aaf5"
        );
        assert!(dash_uuid("069a79f444e94726a5befca90e38aaf").is_err()); // 31 chars, muy corto
        assert!(dash_uuid("not-hex-not-hex-not-hex-not-hex").is_err());
    }

    #[tokio::test]
    async fn login_success_and_invalid_app_registration() -> Result<()> {
        let ok = single_route(
            "/mclogin",
            200,
            r#"{"username":"x","access_token":"mctok","token_type":"Bearer","expires_in":86400}"#
                .into(),
        )
        .await;
        let ep = AuthEndpoints::for_tests(&ok);
        let client = reqwest::Client::new();
        let session = login_with_xbox(&client, &ep, "uhs", "xsts").await?;
        assert_eq!(session.access_token, "mctok");

        let forbidden = single_route(
            "/mclogin",
            403,
            r#"{"path":"/authentication/login_with_xbox","errorType":"FORBIDDEN","error":"FORBIDDEN","errorMessage":"Invalid app registration, see https://aka.ms/AppRegInfo"}"#.into(),
        )
        .await;
        let ep = AuthEndpoints::for_tests(&forbidden);
        let err = login_with_xbox(&client, &ep, "uhs", "xsts")
            .await
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(err.contains("aka.ms/mce-reviewappid"), "{err}");
        Ok(())
    }

    #[tokio::test]
    async fn fetch_profile_parses_active_skin_and_handles_404() -> Result<()> {
        let body = r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","skins":[
            {"id":"a","state":"INACTIVE","url":"http://textures.minecraft.net/texture/old"},
            {"id":"b","state":"ACTIVE","url":"http://textures.minecraft.net/texture/active"}
        ]}"#;
        let base = single_route("/profile", 200, body.into()).await;
        let ep = AuthEndpoints::for_tests(&base);
        let client = reqwest::Client::new();
        let profile = fetch_profile(&client, &ep, "mctok").await?;
        assert_eq!(profile.username, "Notch");
        assert_eq!(profile.uuid, "069a79f4-44e9-4726-a5be-fca90e38aaf5");
        assert_eq!(
            profile.skin_url.as_deref(),
            Some("https://textures.minecraft.net/texture/active")
        );

        let missing = single_route("/profile", 404, String::new()).await;
        let ep = AuthEndpoints::for_tests(&missing);
        let err = fetch_profile(&client, &ep, "mctok")
            .await
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(err.contains("Java Edition"), "{err}");
        Ok(())
    }
}
