//! Cuenta local («offline») para servidores con `online-mode=false`.
//!
//! No habla con Microsoft ni con Xbox Live: el nombre se guarda en disco y el
//! UUID se deriva de forma determinista con el mismo algoritmo que usa el
//! servidor de Minecraft (`MD5("OfflinePlayer:" + nombre)` con bits de versión
//! 3 y variante RFC 4122). Así el jugador conserva inventario y permisos entre
//! sesiones sin depender de una app registrada en Azure.

use md5::{Digest, Md5};

use super::store::Account;
use crate::{error::CoreError, Result};

/// Reglas de nombre de Minecraft: 3-16 caracteres ASCII, letras, números o `_`.
pub fn validate_username(name: &str) -> Result<String> {
    let name = name.trim();
    let len = name.chars().count();
    if !(3..=16).contains(&len) {
        return Err(CoreError::Auth(
            "el nombre debe tener entre 3 y 16 caracteres".into(),
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(CoreError::Auth(
            "el nombre solo puede tener letras, números y guiones bajos".into(),
        ));
    }
    Ok(name.to_owned())
}

/// UUID offline canónico (con guiones), estable para el mismo nombre.
pub fn uuid_for(name: &str) -> String {
    let digest = Md5::digest(format!("OfflinePlayer:{name}").as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest);
    // Versión 3 (MD5) y variante RFC 4122, como hace el servidor.
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// Crea la cuenta local ya validada, lista para guardarse en `account.json`.
pub fn account(name: &str) -> Result<Account> {
    let username = validate_username(name)?;
    Ok(Account {
        uuid: uuid_for(&username),
        username,
        skin_url: None,
        kind: super::store::AccountKind::Offline,
        flow: super::store::AuthFlow::Azure,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_the_same_uuid_as_the_vanilla_server() {
        // Valores comprobados con el algoritmo de referencia de Minecraft.
        assert_eq!(
            uuid_for("Notch"),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
        assert_eq!(
            uuid_for("Steve"),
            "5627dd98-e6be-3c21-b8a8-e92344183641"
        );
        assert_eq!(uuid_for("Notch"), uuid_for("Notch"));
        assert_ne!(uuid_for("Notch"), uuid_for("notch"));
    }

    #[test]
    fn validates_minecraft_username_rules() {
        assert_eq!(validate_username("  Steve_1 ").unwrap_or_default(), "Steve_1");
        for bad in ["", "ab", "con espacio", "acentué", "diecisiete_letras"] {
            assert!(validate_username(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn builds_an_offline_account() {
        let a = account("Nebula_User").expect("cuenta");
        assert_eq!(a.username, "Nebula_User");
        assert_eq!(a.kind, super::super::store::AccountKind::Offline);
        assert!(a.skin_url.is_none());
        assert_eq!(a.uuid.len(), 36);
    }
}
