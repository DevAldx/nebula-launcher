//! Inicio de sesión con cuentas de Microsoft para Minecraft: Java Edition.
//!
//! Hay dos vías:
//! - Oficial: OAuth2 con código de dispositivo usando una app de Azure propia
//!   (ver el README). Es la única aprobada por Mojang.
//! - Heredada (`legacy`): el flujo antiguo de `login.live.com` con el Client ID
//!   público del launcher oficial. No requiere Azure, pero no es una integración
//!   oficial y Microsoft puede bloquearla sin aviso.
//!
//! En ambas, la contraseña se escribe en una página de Microsoft y Nebula solo
//! recibe tokens. Después se encadena Xbox Live → XSTS → Minecraft Services.

mod device_flow;
mod endpoints;
mod legacy;
mod mc;
mod offline;
mod session;
mod store;
mod xbox;

pub use device_flow::{DeviceCode, MsaTokens};
pub use endpoints::{http_client, AuthEndpoints, AUTH_HOSTS};
pub use legacy::LEGACY_CLIENT_ID;
pub use mc::{normalize_skin_url, McSession, Profile};
pub use offline::account as offline_account;
pub use session::{
    begin, begin_legacy, chain_from_legacy_msa, chain_from_msa, resume, wait_for_legacy_sign_in,
    wait_for_sign_in, SignedIn,
};
pub use store::{Account, AccountKind, AccountStore, AuthFlow, SecretStore};
