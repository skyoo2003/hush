// SPDX-License-Identifier: GPL-3.0-or-later
//! The DB key and where it lives. On macOS it lives in the login keychain.

use std::sync::Mutex;

use rand_core::{OsRng, TryRngCore};
use zeroize::Zeroizing;

use super::error::StorageError;

/// 32 raw bytes given to SQLCipher as a raw hex key, which skips its PBKDF2 step.
/// Deliberately has no Debug, Display, or Clone.
pub struct DbKey(Zeroizing<[u8; 32]>);

impl DbKey {
    pub fn generate() -> Result<Self, StorageError> {
        let mut key = Zeroizing::new([0u8; 32]);
        OsRng
            .try_fill_bytes(key.as_mut())
            .map_err(|_| StorageError::Rng)?;
        Ok(Self(key))
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self, StorageError> {
        let key: [u8; 32] = bytes.try_into().map_err(|_| StorageError::MalformedKey)?;
        Ok(Self(Zeroizing::new(key)))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// `PRAGMA key = "x'<64 hex>'";`, built in a buffer sized up front so no reallocation leaves a copy.
    pub(super) fn pragma(&self) -> Zeroizing<String> {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let (head, tail) = ("PRAGMA key = \"x'", "'\";");
        let mut s = Zeroizing::new(String::with_capacity(head.len() + 64 + tail.len()));
        s.push_str(head);
        for b in self.0.iter() {
            s.push(HEX[usize::from(b >> 4)] as char);
            s.push(HEX[usize::from(b & 0xf)] as char);
        }
        s.push_str(tail);
        s
    }
}

pub trait KeyProvider: Send + Sync {
    fn load(&self) -> Result<Option<DbKey>, StorageError>;
    /// Creates or overwrites the stored key.
    fn store(&self, key: &DbKey) -> Result<(), StorageError>;
    /// Removes the stored key. A missing key is not an error.
    fn delete(&self) -> Result<(), StorageError>;
}

/// Login keychain item: service `brand::KEYCHAIN_SERVICE`, account `db-key-v1`.
#[cfg(target_os = "macos")]
pub struct KeychainKeyProvider;

#[cfg(target_os = "macos")]
impl KeychainKeyProvider {
    const ACCOUNT: &'static str = "db-key-v1";
    const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
}

#[cfg(target_os = "macos")]
impl KeyProvider for KeychainKeyProvider {
    fn load(&self) -> Result<Option<DbKey>, StorageError> {
        use security_framework::passwords::get_generic_password;
        match get_generic_password(crate::brand::KEYCHAIN_SERVICE, Self::ACCOUNT) {
            Ok(bytes) => DbKey::from_slice(&Zeroizing::new(bytes)).map(Some),
            Err(e) if e.code() == Self::ERR_SEC_ITEM_NOT_FOUND => Ok(None),
            Err(e) => Err(StorageError::Keychain(e.code())),
        }
    }

    fn store(&self, key: &DbKey) -> Result<(), StorageError> {
        security_framework::passwords::set_generic_password(
            crate::brand::KEYCHAIN_SERVICE,
            Self::ACCOUNT,
            key.as_bytes(),
        )
        .map_err(|e| StorageError::Keychain(e.code()))
    }

    fn delete(&self) -> Result<(), StorageError> {
        match security_framework::passwords::delete_generic_password(
            crate::brand::KEYCHAIN_SERVICE,
            Self::ACCOUNT,
        ) {
            Err(e) if e.code() != Self::ERR_SEC_ITEM_NOT_FOUND => {
                Err(StorageError::Keychain(e.code()))
            }
            _ => Ok(()),
        }
    }
}

#[derive(Default)]
pub struct InMemoryKeyProvider(Mutex<Option<Zeroizing<[u8; 32]>>>);

impl KeyProvider for InMemoryKeyProvider {
    fn load(&self) -> Result<Option<DbKey>, StorageError> {
        let slot = self.0.lock().unwrap_or_else(|e| e.into_inner());
        slot.as_deref().map(|k| DbKey::from_slice(k)).transpose()
    }

    fn store(&self, key: &DbKey) -> Result<(), StorageError> {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(Zeroizing::new(*key.as_bytes()));
        Ok(())
    }

    fn delete(&self) -> Result<(), StorageError> {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_keys_differ_and_pragma_is_raw_hex() {
        let (a, b) = (DbKey::generate().unwrap(), DbKey::generate().unwrap());
        assert_ne!(a.as_bytes(), b.as_bytes());
        let k = DbKey::from_slice(&[0xab; 32]).unwrap();
        assert_eq!(
            *k.pragma(),
            format!("PRAGMA key = \"x'{}'\";", "ab".repeat(32))
        );
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(matches!(
            DbKey::from_slice(&[0; 31]),
            Err(StorageError::MalformedKey)
        ));
    }

    #[test]
    fn in_memory_round_trip() {
        let p = InMemoryKeyProvider::default();
        assert!(p.load().unwrap().is_none());
        p.store(&DbKey::from_slice(&[7; 32]).unwrap()).unwrap();
        assert_eq!(p.load().unwrap().unwrap().as_bytes(), &[7; 32]);
    }
}
