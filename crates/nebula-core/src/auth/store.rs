//! Guarda el refresh token de Microsoft en el almacén seguro del sistema operativo
//! (Secret Service / kwallet en Linux vía la crate `keyring`) y cachea el perfil
//! (UUID, nombre, skin) en disco, sin nada sensible, para mostrarlo sin esperar red.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::mc::Profile;
use crate::{error::CoreError, filesystem, NebulaPaths, Result};

const SERVICE: &str = "dev.nebula.launcher";
const USER: &str = "microsoft-refresh-token";

/// Origen de la cuenta guardada. Las cuentas locales no tienen refresh token:
/// solo sirven en servidores con `online-mode=false`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountKind {
    #[default]
    Microsoft,
    Offline,
}

/// Cómo se obtuvo la sesión de Microsoft. `Azure` es el flujo oficial con una
/// app propia; `Legacy` es el flujo antiguo de `login.live.com` con el Client ID
/// público del launcher oficial (no oficial, pero sin necesidad de Azure).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthFlow {
    #[default]
    Azure,
    Legacy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub uuid: String,
    pub username: String,
    pub skin_url: Option<String>,
    /// `default` para que las cuentas de versiones anteriores sigan cargando.
    #[serde(default)]
    pub kind: AccountKind,
    /// Flujo que hay que usar para renovar el refresh token guardado.
    #[serde(default)]
    pub flow: AuthFlow,
}

impl From<Profile> for Account {
    fn from(p: Profile) -> Self {
        Self {
            uuid: p.uuid,
            username: p.username,
            skin_url: p.skin_url,
            kind: AccountKind::Microsoft,
            flow: AuthFlow::Azure,
        }
    }
}

/// Backend de almacenamiento seguro para un único secreto (el refresh token).
/// Separado de `AccountStore` para poder probar la lógica de caché sin depender
/// de un Secret Service real en el entorno donde corren los tests.
pub trait SecretStore: Send + Sync {
    fn get(&self) -> Result<Option<String>>;
    fn set(&self, value: &str) -> Result<()>;
    fn clear(&self) -> Result<()>;
}

pub struct KeyringSecretStore;

impl KeyringSecretStore {
    fn entry(&self) -> Result<keyring::Entry> {
        keyring::Entry::new(SERVICE, USER).map_err(|e| {
            CoreError::Auth(format!(
                "no se pudo acceder al almacén seguro de credenciales del sistema: {e}"
            ))
        })
    }
}

impl SecretStore for KeyringSecretStore {
    fn get(&self) -> Result<Option<String>> {
        match self.entry()?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CoreError::Auth(format!(
                "no se pudo leer la sesión guardada: {e}"
            ))),
        }
    }

    fn set(&self, value: &str) -> Result<()> {
        self.entry()?
            .set_password(value)
            .map_err(|e| CoreError::Auth(format!("no se pudo guardar la sesión: {e}")))
    }

    fn clear(&self) -> Result<()> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(CoreError::Auth(format!(
                "no se pudo borrar la sesión guardada: {e}"
            ))),
        }
    }
}

pub struct AccountStore {
    cache_path: PathBuf,
    secret: Box<dyn SecretStore>,
}

impl AccountStore {
    pub fn new(paths: &NebulaPaths) -> Self {
        Self {
            cache_path: paths.accounts_dir().join("account.json"),
            secret: Box::new(KeyringSecretStore),
        }
    }

    #[cfg(test)]
    pub(super) fn with_store(cache_path: PathBuf, secret: Box<dyn SecretStore>) -> Self {
        Self { cache_path, secret }
    }

    /// Último perfil conocido, sin tocar la red. Puede estar desactualizado;
    /// úsalo solo para mostrar algo mientras se confirma la sesión de verdad.
    pub fn cached_account(&self) -> Option<Account> {
        let bytes = std::fs::read(&self.cache_path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn refresh_token(&self) -> Result<Option<String>> {
        self.secret.get()
    }

    pub fn save(&self, refresh_token: &str, account: &Account) -> Result<()> {
        self.secret.set(refresh_token)?;
        self.write_cache(account)
    }

    /// Guarda una cuenta local. No necesita el almacén seguro del sistema y,
    /// si había una sesión de Microsoft, la elimina para no mezclar cuentas.
    pub fn save_offline(&self, account: &Account) -> Result<()> {
        let _ = self.secret.clear();
        self.write_cache(account)
    }

    fn write_cache(&self, account: &Account) -> Result<()> {
        let json = serde_json::to_vec_pretty(account).map_err(|e| CoreError::Json {
            path: self.cache_path.clone(),
            source: e,
        })?;
        filesystem::write_atomic(&self.cache_path, &json)
    }

    pub fn clear(&self) -> Result<()> {
        self.secret.clear()?;
        let _ = std::fs::remove_file(&self.cache_path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::test_util::TempDir;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemorySecretStore(Mutex<Option<String>>);
    impl SecretStore for MemorySecretStore {
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

    fn account(name: &str) -> Account {
        Account {
            uuid: "069a79f4-44e9-4726-a5be-fca90e38aaf5".into(),
            username: name.into(),
            skin_url: Some("https://x/skin.png".into()),
            kind: AccountKind::Microsoft,
            flow: AuthFlow::Azure,
        }
    }

    #[test]
    fn starts_empty_then_round_trips_and_clears() {
        let tmp = TempDir::new();
        let store = AccountStore::with_store(
            tmp.path().join("account.json"),
            Box::new(MemorySecretStore::default()),
        );
        assert!(store.cached_account().is_none());
        assert_eq!(store.refresh_token().unwrap_or(None), None);

        store.save("rt-123", &account("Notch")).expect("save");
        assert_eq!(store.refresh_token().unwrap_or(None), Some("rt-123".into()));
        assert_eq!(store.cached_account(), Some(account("Notch")));

        // Guardar de nuevo con otra cuenta sobrescribe, no acumula.
        store.save("rt-456", &account("Jeb_")).expect("save 2");
        assert_eq!(store.refresh_token().unwrap_or(None), Some("rt-456".into()));
        assert_eq!(store.cached_account(), Some(account("Jeb_")));

        store.clear().expect("clear");
        assert!(store.cached_account().is_none());
        assert_eq!(store.refresh_token().unwrap_or(None), None);
    }

    #[test]
    fn offline_account_has_no_refresh_token() {
        let tmp = TempDir::new();
        let store = AccountStore::with_store(
            tmp.path().join("account.json"),
            Box::new(MemorySecretStore::default()),
        );
        store.save("rt-viejo", &account("Notch")).expect("msa");
        let mut local = account("Local_Player");
        local.kind = AccountKind::Offline;
        local.skin_url = None;
        store.save_offline(&local).expect("offline");
        assert_eq!(store.cached_account(), Some(local));
        // La sesión de Microsoft anterior deja de estar disponible.
        assert_eq!(store.refresh_token().unwrap_or(None), None);
    }
}
