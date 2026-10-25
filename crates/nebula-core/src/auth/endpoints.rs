//! URLs del flujo Microsoft → Xbox Live → XSTS → Minecraft, con lista blanca de hosts.

use reqwest::Url;

use crate::{error::CoreError, Result};

/// Hosts oficiales usados por el flujo de autenticación.
pub const AUTH_HOSTS: &[&str] = &[
    "login.microsoftonline.com",
    "login.live.com",
    "user.auth.xboxlive.com",
    "xsts.auth.xboxlive.com",
    "api.minecraftservices.com",
];

#[derive(Debug, Clone)]
pub struct AuthEndpoints {
    pub device_code_url: String,
    pub token_url: String,
    pub legacy_device_code_url: String,
    pub legacy_token_url: String,
    pub xbl_url: String,
    pub xsts_url: String,
    pub mc_login_url: String,
    pub mc_profile_url: String,
    allowed_hosts: Vec<String>,
    https_only: bool,
}

impl AuthEndpoints {
    /// Endpoints reales de Microsoft/Xbox/Mojang. El tenant `consumers` es obligatorio:
    /// `common` o un tenant de empresa no sirven para cuentas de Minecraft.
    pub fn live() -> Self {
        Self {
            device_code_url: "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode"
                .into(),
            token_url: "https://login.microsoftonline.com/consumers/oauth2/v2.0/token".into(),
            legacy_device_code_url: "https://login.live.com/oauth20_connect.srf".into(),
            legacy_token_url: "https://login.live.com/oauth20_token.srf".into(),
            xbl_url: "https://user.auth.xboxlive.com/user/authenticate".into(),
            xsts_url: "https://xsts.auth.xboxlive.com/xsts/authorize".into(),
            mc_login_url: "https://api.minecraftservices.com/authentication/login_with_xbox"
                .into(),
            mc_profile_url: "https://api.minecraftservices.com/minecraft/profile".into(),
            allowed_hosts: AUTH_HOSTS.iter().map(|s| (*s).to_owned()).collect(),
            https_only: true,
        }
    }

    #[cfg(test)]
    pub fn for_tests(base: &str) -> Self {
        Self {
            device_code_url: format!("{base}/devicecode"),
            token_url: format!("{base}/token"),
            legacy_device_code_url: format!("{base}/legacy_device_code"),
            legacy_token_url: format!("{base}/legacy_token"),
            xbl_url: format!("{base}/xbl"),
            xsts_url: format!("{base}/xsts"),
            mc_login_url: format!("{base}/mclogin"),
            mc_profile_url: format!("{base}/profile"),
            allowed_hosts: vec!["127.0.0.1".to_owned()],
            https_only: false,
        }
    }

    /// Valida esquema y host antes de cada petición: ni un redirect ni una URL
    /// mal construida pueden sacar una llamada de autenticación de la lista blanca.
    pub(crate) fn check(&self, url: &str) -> Result<Url> {
        let parsed =
            Url::parse(url).map_err(|e| CoreError::Network(format!("URL inválida: {e}")))?;
        if self.https_only && parsed.scheme() != "https" {
            return Err(CoreError::Network(format!("solo HTTPS permitido: {url}")));
        }
        let host = parsed.host_str().unwrap_or_default();
        if !self.allowed_hosts.iter().any(|h| h == host) {
            return Err(CoreError::Network(format!("host no permitido: {host}")));
        }
        Ok(parsed)
    }
}

/// Cliente HTTP para el flujo de autenticación: solo HTTPS, sin seguir redirects
/// (un redirect inesperado debe fallar de forma visible, no saltar de host en silencio).
pub fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!("NebulaLauncher/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| CoreError::Network(e.to_string()))
}
