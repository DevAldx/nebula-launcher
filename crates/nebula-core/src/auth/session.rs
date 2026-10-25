//! Orquesta el flujo completo: código de dispositivo → Xbox Live → XSTS →
//! Minecraft → perfil. Separado en pasos pequeños (`device_flow`, `xbox`, `mc`)
//! para que cada uno se pueda probar de forma aislada contra un servidor falso.

use std::time::{Duration, Instant};

use tokio::sync::oneshot;

use super::{
    device_flow::{self, DeviceCode, MsaTokens, PollStep},
    endpoints::AuthEndpoints,
    legacy,
    mc::{self, McSession, Profile},
    store::{Account, AccountStore, AuthFlow},
    xbox,
};
use crate::{error::CoreError, Result};

pub struct SignedIn {
    pub account: Account,
    pub session: McSession,
}

/// El Client ID debe ser el GUID de una aplicación de Azure propia
/// (Portal de Azure → App registrations), no una cadena arbitraria.
fn validate_client_id(id: &str) -> Result<()> {
    let id = id.trim();
    let groups: Vec<&str> = id.split('-').collect();
    let looks_like_guid = matches!(groups.as_slice(), [a, b, c, d, e]
        if [a.len(), b.len(), c.len(), d.len(), e.len()] == [8, 4, 4, 4, 12])
        && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    if looks_like_guid {
        Ok(())
    } else {
        Err(CoreError::Auth(
            "configura un Client ID de Microsoft válido en Configuración > Cuenta (el GUID de \
             tu propia aplicación de Azure, no el de otro launcher)."
                .into(),
        ))
    }
}

pub async fn begin(
    client: &reqwest::Client,
    ep: &AuthEndpoints,
    client_id: &str,
) -> Result<DeviceCode> {
    validate_client_id(client_id)?;
    device_flow::request_device_code(client, ep, client_id).await
}

/// Código de dispositivo del flujo heredado. No valida ningún Client ID porque
/// usa el identificador público del launcher oficial.
pub async fn begin_legacy(client: &reqwest::Client, ep: &AuthEndpoints) -> Result<DeviceCode> {
    legacy::request_device_code(client, ep).await
}

/// Espera a que el usuario complete el inicio de sesión en su navegador.
/// Duerme `interval` entre sondeos (como pide la especificación OAuth2) y
/// termina antes de tiempo si `cancel` se dispara o el código expira.
pub async fn wait_for_sign_in(
    client: &reqwest::Client,
    ep: &AuthEndpoints,
    client_id: &str,
    code: &DeviceCode,
    cancel: oneshot::Receiver<()>,
) -> Result<MsaTokens> {
    wait_for_sign_in_inner(client, ep, Some(client_id), code, cancel).await
}

/// Igual que `wait_for_sign_in`, pero sondeando el endpoint heredado.
pub async fn wait_for_legacy_sign_in(
    client: &reqwest::Client,
    ep: &AuthEndpoints,
    code: &DeviceCode,
    cancel: oneshot::Receiver<()>,
) -> Result<MsaTokens> {
    wait_for_sign_in_inner(client, ep, None, code, cancel).await
}

async fn wait_for_sign_in_inner(
    client: &reqwest::Client,
    ep: &AuthEndpoints,
    client_id: Option<&str>,
    code: &DeviceCode,
    mut cancel: oneshot::Receiver<()>,
) -> Result<MsaTokens> {
    let mut interval = code.interval;
    loop {
        if Instant::now() >= code.expires_at {
            return Err(CoreError::Auth(
                "el código expiró antes de completar el inicio de sesión; vuelve a intentarlo."
                    .into(),
            ));
        }
        tokio::select! {
            _ = &mut cancel => {
                return Err(CoreError::Auth("inicio de sesión cancelado.".into()));
            }
            () = tokio::time::sleep(interval) => {}
        }
        let step = match client_id {
            Some(id) => device_flow::poll_once(client, ep, id, &code.device_code).await?,
            None => legacy::poll_once(client, ep, &code.device_code).await?,
        };
        match step {
            PollStep::Pending => continue,
            PollStep::SlowDown => {
                interval += Duration::from_secs(5);
                continue;
            }
            PollStep::Success(tokens) => return Ok(tokens),
        }
    }
}

/// A partir de un access token de Microsoft ya válido, completa Xbox Live → XSTS
/// → Minecraft → perfil. Usado tanto justo tras el login como en cada renovación.
pub async fn chain_from_msa(
    client: &reqwest::Client,
    ep: &AuthEndpoints,
    msa_access_token: &str,
) -> Result<(McSession, Profile)> {
    chain_from_msa_with_ticket(client, ep, msa_access_token, "d").await
}

/// Variante para tokens del flujo heredado: Xbox Live espera el prefijo `t=`.
pub async fn chain_from_legacy_msa(
    client: &reqwest::Client,
    ep: &AuthEndpoints,
    msa_access_token: &str,
) -> Result<(McSession, Profile)> {
    chain_from_msa_with_ticket(client, ep, msa_access_token, "t").await
}

async fn chain_from_msa_with_ticket(
    client: &reqwest::Client,
    ep: &AuthEndpoints,
    msa_access_token: &str,
    ticket_prefix: &str,
) -> Result<(McSession, Profile)> {
    let xbl = xbox::authenticate_xbl(client, ep, msa_access_token, ticket_prefix).await?;
    let xsts = xbox::authorize_xsts(client, ep, &xbl.token).await?;
    let session = mc::login_with_xbox(client, ep, &xsts.uhs, &xsts.token).await?;
    let profile = mc::fetch_profile(client, ep, &session.access_token).await?;
    Ok((session, profile))
}

/// Usa el refresh token guardado para obtener una sesión de Minecraft al vuelo.
/// `Ok(None)` significa que no hay ninguna cuenta guardada (modo demo).
/// Si el refresh token ya no es válido, borra la cuenta guardada y lo explica
/// en el error para que la UI pueda ofrecer iniciar sesión de nuevo.
pub async fn resume(
    client: &reqwest::Client,
    ep: &AuthEndpoints,
    client_id: &str,
    store: &AccountStore,
) -> Result<Option<SignedIn>> {
    let Some(refresh_token) = store.refresh_token()? else {
        return Ok(None);
    };
    // El flujo se guarda junto al perfil: las cuentas heredadas renuevan contra
    // login.live.com y no necesitan el Client ID de Azure.
    let flow = store
        .cached_account()
        .map(|account| account.flow)
        .unwrap_or_default();
    let tokens = match flow {
        AuthFlow::Legacy => legacy::refresh(client, ep, &refresh_token).await,
        AuthFlow::Azure => {
            validate_client_id(client_id)?;
            device_flow::refresh(client, ep, client_id, &refresh_token).await
        }
    };
    let tokens = match tokens {
        Ok(t) => t,
        Err(e) => {
            store.clear()?;
            return Err(CoreError::Auth(format!(
                "tu sesión de Microsoft expiró; inicia sesión de nuevo. ({e})"
            )));
        }
    };
    let (session, profile) = match flow {
        AuthFlow::Legacy => chain_from_legacy_msa(client, ep, &tokens.access_token).await?,
        AuthFlow::Azure => chain_from_msa(client, ep, &tokens.access_token).await?,
    };
    let mut account: Account = profile.into();
    account.flow = flow;
    store.save(&tokens.refresh_token, &account)?;
    Ok(Some(SignedIn { account, session }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::test_util::TempDir;
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    /// Servidor con una ruta por paso del flujo, cada una devuelve siempre el
    /// mismo cuerpo salvo `/token`, que puede servir una secuencia de respuestas
    /// (para simular "authorization_pending" antes del éxito).
    async fn fake_auth_server(
        routes: HashMap<&'static str, String>,
        token_sequence: Vec<(u16, String)>,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        let routes = Arc::new(routes);
        let token_idx = Arc::new(Mutex::new(0usize));
        let token_sequence = Arc::new(token_sequence);
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let routes = routes.clone();
                let token_idx = token_idx.clone();
                let token_sequence = token_sequence.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let path = req.split_whitespace().nth(1).unwrap_or("/");
                    let (status, body) = if path == "/token" && !token_sequence.is_empty() {
                        let mut i = token_idx.lock().unwrap_or_else(|e| e.into_inner());
                        let step = token_sequence[(*i).min(token_sequence.len() - 1)].clone();
                        *i += 1;
                        step
                    } else if let Some(b) = routes.get(path) {
                        (200, b.clone())
                    } else {
                        (404, String::new())
                    };
                    let line = if status == 200 { "200 OK" } else { "400 Bad Request" };
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

    const GUID: &str = "11111111-2222-3333-4444-555555555555";

    fn full_routes() -> HashMap<&'static str, String> {
        let mut routes = HashMap::new();
        routes.insert(
            "/xbl",
            r#"{"Token":"xbltok","DisplayClaims":{"xui":[{"uhs":"hash1"}]}}"#.to_owned(),
        );
        routes.insert(
            "/xsts",
            r#"{"Token":"xststok","DisplayClaims":{"xui":[{"uhs":"hash1"}]}}"#.to_owned(),
        );
        routes.insert(
            "/mclogin",
            r#"{"access_token":"mctok","expires_in":86400}"#.to_owned(),
        );
        routes.insert(
            "/profile",
            r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","skins":[{"id":"a","state":"ACTIVE","url":"https://x/skin.png"}]}"#.to_owned(),
        );
        routes
    }

    #[test]
    fn validates_client_id_shape() {
        assert!(validate_client_id(GUID).is_ok());
        assert!(validate_client_id("00000000402b5328").is_err()); // id legado, no un GUID
        assert!(validate_client_id("").is_err());
        assert!(validate_client_id("not a guid at all").is_err());
    }

    #[tokio::test]
    async fn chain_from_msa_wires_all_four_steps() -> Result<()> {
        let base = fake_auth_server(full_routes(), vec![]).await;
        let ep = AuthEndpoints::for_tests(&base);
        let client = reqwest::Client::new();
        let (session, profile) = chain_from_msa(&client, &ep, "msa-access-token").await?;
        assert_eq!(session.access_token, "mctok");
        assert_eq!(profile.username, "Notch");
        assert_eq!(profile.uuid, "069a79f4-44e9-4726-a5be-fca90e38aaf5");
        Ok(())
    }

    #[tokio::test]
    async fn wait_for_sign_in_retries_pending_then_succeeds() -> Result<()> {
        let sequence = vec![
            (400, r#"{"error":"authorization_pending"}"#.to_owned()),
            (400, r#"{"error":"authorization_pending"}"#.to_owned()),
            (
                200,
                r#"{"access_token":"at","refresh_token":"rt"}"#.to_owned(),
            ),
        ];
        let base = fake_auth_server(HashMap::new(), sequence).await;
        let ep = AuthEndpoints::for_tests(&base);
        let client = reqwest::Client::new();
        let code = DeviceCode {
            device_code: "dc".into(),
            user_code: "ABCD".into(),
            verification_uri: "https://www.microsoft.com/link".into(),
            interval: Duration::from_millis(1),
            expires_at: Instant::now() + Duration::from_secs(30),
        };
        let (_tx, rx) = oneshot::channel();
        let tokens = wait_for_sign_in(&client, &ep, GUID, &code, rx).await?;
        assert_eq!(tokens.access_token, "at");
        Ok(())
    }

    #[tokio::test]
    async fn wait_for_sign_in_can_be_cancelled() -> Result<()> {
        let sequence = vec![(400, r#"{"error":"authorization_pending"}"#.to_owned())];
        let base = fake_auth_server(HashMap::new(), sequence).await;
        let ep = AuthEndpoints::for_tests(&base);
        let client = reqwest::Client::new();
        let code = DeviceCode {
            device_code: "dc".into(),
            user_code: "ABCD".into(),
            verification_uri: "https://www.microsoft.com/link".into(),
            interval: Duration::from_secs(5),
            expires_at: Instant::now() + Duration::from_secs(30),
        };
        let (tx, rx) = oneshot::channel();
        let task = tokio::spawn(async move { wait_for_sign_in(&client, &ep, GUID, &code, rx).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = tx.send(());
        let result = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .map_err(|_| CoreError::Auth("no terminó tras cancelar".into()))?
            .map_err(|_| CoreError::Auth("el task de sign-in falló".into()))?;
        assert!(result.is_err());
        Ok(())
    }

    #[derive(Default)]
    struct EmptySecretStore;
    impl super::super::store::SecretStore for EmptySecretStore {
        fn get(&self) -> Result<Option<String>> {
            Ok(None)
        }
        fn set(&self, _value: &str) -> Result<()> {
            Ok(())
        }
        fn clear(&self) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn resume_without_saved_account_is_none() -> Result<()> {
        let tmp = TempDir::new();
        let store = AccountStore::with_store(tmp.path().join("account.json"), Box::new(EmptySecretStore));
        let ep = AuthEndpoints::for_tests("http://127.0.0.1:1");
        let client = reqwest::Client::new();
        let result = resume(&client, &ep, GUID, &store).await?;
        assert!(result.is_none());
        Ok(())
    }

    struct MemorySecretStore(Mutex<Option<String>>);
    impl super::super::store::SecretStore for MemorySecretStore {
        fn get(&self) -> Result<Option<String>> {
            Ok(self.0.lock().ok().and_then(|g| g.clone()))
        }
        fn set(&self, value: &str) -> Result<()> {
            if let Ok(mut g) = self.0.lock() {
                *g = Some(value.to_owned());
            }
            Ok(())
        }
        fn clear(&self) -> Result<()> {
            if let Ok(mut g) = self.0.lock() {
                *g = None;
            }
            Ok(())
        }
    }

    /// Una cuenta heredada debe renovar contra `login.live.com` y no exigir un
    /// Client ID de Azure (que aquí llega vacío).
    #[tokio::test]
    async fn resume_legacy_uses_the_legacy_refresh() -> Result<()> {
        let mut routes = full_routes();
        routes.insert(
            "/legacy_token",
            r#"{"access_token":"new-at","refresh_token":"new-rt"}"#.to_owned(),
        );
        let base = fake_auth_server(routes, vec![]).await;
        let ep = AuthEndpoints::for_tests(&base);
        let client = reqwest::Client::new();
        let tmp = TempDir::new();
        let store = AccountStore::with_store(
            tmp.path().join("account.json"),
            Box::new(MemorySecretStore(Mutex::new(None))),
        );
        let account = Account {
            uuid: "069a79f4-44e9-4726-a5be-fca90e38aaf5".into(),
            username: "Notch".into(),
            skin_url: None,
            kind: crate::auth::AccountKind::Microsoft,
            flow: AuthFlow::Legacy,
        };
        store.save("old-rt", &account)?;

        let signed_in = resume(&client, &ep, "", &store)
            .await?
            .expect("cuenta heredada");
        assert_eq!(signed_in.account.username, "Notch");
        assert_eq!(signed_in.account.flow, AuthFlow::Legacy);
        assert_eq!(store.refresh_token()?, Some("new-rt".into()));
        Ok(())
    }
}
