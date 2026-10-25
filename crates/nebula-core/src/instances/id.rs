use std::fmt;

use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize};

use crate::{error::CoreError, Result};

const PREFIX: &str = "NBL-";
const SUFFIX_LEN: usize = 6;
const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Identificador de instancia con formato `NBL-XXXXXX` (X ∈ A-Z0-9).
///
/// Es el único valor que se usa como nombre de carpeta, y su validación estricta
/// hace imposible el path traversal a través de un ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct InstanceId(String);

impl InstanceId {
    pub fn generate() -> Self {
        let mut rng = rand::thread_rng();
        let suffix: String = (0..SUFFIX_LEN)
            .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
            .collect();
        Self(format!("{PREFIX}{suffix}"))
    }

    pub fn parse(s: &str) -> Result<Self> {
        let valid = s.len() == PREFIX.len() + SUFFIX_LEN
            && s.starts_with(PREFIX)
            && s[PREFIX.len()..]
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
        if valid {
            Ok(Self(s.to_owned()))
        } else {
            Err(CoreError::invalid(format!(
                "ID de instancia inválido: {s:?} (formato NBL-XXXXXX)"
            )))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for InstanceId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_ids_are_valid() {
        for _ in 0..200 {
            let id = InstanceId::generate();
            assert!(InstanceId::parse(id.as_str()).is_ok(), "{id}");
        }
    }

    #[test]
    fn parse_accepts_spec_examples() {
        assert!(InstanceId::parse("NBL-A91K2X").is_ok());
        assert!(InstanceId::parse("NBL-X72PQ9").is_ok());
    }

    #[test]
    fn parse_rejects_malformed_and_traversal() {
        for bad in [
            "", "NBL-", "NBL-abc123", "NBL-A91K2", "NBL-A91K2XX", "XBL-A91K2X",
            "NBL-../../x", "NBL-A91K2/", "../NBL-A91K2X", "NBL-A91K2\n",
        ] {
            assert!(InstanceId::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn deserialize_validates() {
        assert!(serde_json::from_str::<InstanceId>("\"NBL-A91K2X\"").is_ok());
        assert!(serde_json::from_str::<InstanceId>("\"../etc\"").is_err());
    }
}
