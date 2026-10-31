//! Secret storage.
//!
//! Design (see PLAN.md, review finding 5): the OS keyring (macOS Keychain, Windows Credential
//! Manager, Linux Secret Service) only holds a random 32-byte master key. Everything else
//! (session cookies, tokens, passwords) is stored XChaCha20-Poly1305-encrypted in `secrets.enc`
//! inside the user's config directory. This avoids the 2.5 KB blob limit of the Windows
//! Credential Manager and keeps the on-disk file unreadable without the keyring.
//!
//! If no keyring is available (e.g. Linux without a Secret Service), the master key is kept in a
//! file with owner-only permissions next to the secrets file; the UI shows a warning in that case.

use crate::error::{Error, Result};
use crate::models::Credential;
use crate::settings::AppPaths;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

const SERVICE: &str = "de.yattt.app";
const MASTER_KEY_ACCOUNT: &str = "master-key";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SecretBackend {
    /// Master key in the OS keyring, payload encrypted on disk.
    Keyring,
    /// Master key in a 0600 file, payload encrypted on disk. Weaker, but works everywhere.
    File,
}

pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn set(&self, key: &str, value: &str) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
    fn backend(&self) -> SecretBackend;
}

#[derive(Debug, Serialize, Deserialize)]
struct EncryptedFile {
    v: u8,
    nonce: String,
    ciphertext: String,
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

/// Where the master key comes from.
enum KeySource {
    Keyring,
    File(PathBuf),
}

pub struct EncryptedSecretStore {
    secrets_file: PathBuf,
    key_source: KeySource,
    key: Mutex<Option<[u8; 32]>>,
}

impl EncryptedSecretStore {
    /// Uses the keyring when available, otherwise a key file.
    pub fn new(paths: &AppPaths) -> Self {
        let key_file = paths.config_dir.join("secrets.key");
        let key_source = if keyring_usable() {
            KeySource::Keyring
        } else {
            KeySource::File(key_file)
        };
        Self {
            secrets_file: paths.config_dir.join("secrets.enc"),
            key_source,
            key: Mutex::new(None),
        }
    }

    /// Always uses a key file. Used in tests and on systems without a keyring.
    pub fn file_only(paths: &AppPaths) -> Self {
        Self {
            secrets_file: paths.config_dir.join("secrets.enc"),
            key_source: KeySource::File(paths.config_dir.join("secrets.key")),
            key: Mutex::new(None),
        }
    }

    fn master_key(&self) -> Result<[u8; 32]> {
        if let Some(k) = *self.key.lock().unwrap() {
            return Ok(k);
        }
        let key = match &self.key_source {
            KeySource::Keyring => {
                let entry = keyring::Entry::new(SERVICE, MASTER_KEY_ACCOUNT)?;
                match entry.get_password() {
                    Ok(encoded) => decode_key(&encoded)?,
                    Err(keyring::Error::NoEntry) => {
                        let key = generate_key();
                        entry.set_password(&b64().encode(key))?;
                        key
                    }
                    Err(e) => return Err(Error::Keyring(e.to_string())),
                }
            }
            KeySource::File(path) => {
                if path.exists() {
                    decode_key(std::fs::read_to_string(path)?.trim())?
                } else {
                    let key = generate_key();
                    write_private(path, b64().encode(key).as_bytes())?;
                    key
                }
            }
        };
        *self.key.lock().unwrap() = Some(key);
        Ok(key)
    }

    fn read_all(&self) -> Result<BTreeMap<String, String>> {
        if !self.secrets_file.exists() {
            return Ok(BTreeMap::new());
        }
        let raw = std::fs::read_to_string(&self.secrets_file)?;
        let file: EncryptedFile = serde_json::from_str(&raw)?;
        let key = self.master_key()?;
        let cipher = XChaCha20Poly1305::new((&key).into());
        let nonce_bytes = b64()
            .decode(&file.nonce)
            .map_err(|e| Error::Parse(e.to_string()))?;
        let ciphertext = b64()
            .decode(&file.ciphertext)
            .map_err(|e| Error::Parse(e.to_string()))?;
        let nonce = XNonce::from_slice(&nonce_bytes);
        let plain = cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|_| Error::Keyring(crate::tr!("Gespeicherte Zugangsdaten konnten nicht entschlüsselt werden (Schlüssel geändert?)", "Stored credentials could not be decrypted (key changed?)").into()))?;
        Ok(serde_json::from_slice(&plain)?)
    }

    fn write_all(&self, map: &BTreeMap<String, String>) -> Result<()> {
        let key = self.master_key()?;
        let cipher = XChaCha20Poly1305::new((&key).into());
        let mut nonce_bytes = [0u8; 24];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce_bytes);
        let nonce = XNonce::from_slice(&nonce_bytes);
        let plain = serde_json::to_vec(map)?;
        let ciphertext = cipher.encrypt(nonce, plain.as_ref()).map_err(|_| {
            Error::Keyring(crate::tr!("Verschlüsselung fehlgeschlagen", "Encryption failed").into())
        })?;
        let file = EncryptedFile {
            v: 1,
            nonce: b64().encode(nonce_bytes),
            ciphertext: b64().encode(ciphertext),
        };
        write_private(&self.secrets_file, serde_json::to_string(&file)?.as_bytes())
    }
}

fn generate_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut key);
    key
}

fn decode_key(encoded: &str) -> Result<[u8; 32]> {
    let bytes = b64().decode(encoded).map_err(|e| {
        Error::Keyring(crate::tr!(
            "Master-Key ungültig: {e}",
            "Master key invalid: {e}",
            e = e
        ))
    })?;
    bytes.try_into().map_err(|_| {
        Error::Keyring(
            crate::tr!(
                "Master-Key hat falsche Länge",
                "Master key has the wrong length"
            )
            .into(),
        )
    })
}

fn write_private(path: &std::path::Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn keyring_usable() -> bool {
    match keyring::Entry::new(SERVICE, "__probe__") {
        Ok(entry) => match entry.get_password() {
            Ok(_) | Err(keyring::Error::NoEntry) => true,
            Err(keyring::Error::PlatformFailure(_)) | Err(keyring::Error::NoStorageAccess(_)) => {
                false
            }
            Err(_) => true,
        },
        Err(_) => false,
    }
}

impl SecretStore for EncryptedSecretStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.read_all()?.get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        let mut map = self.read_all()?;
        map.insert(key.to_string(), value.to_string());
        self.write_all(&map)
    }

    fn delete(&self, key: &str) -> Result<()> {
        let mut map = self.read_all()?;
        if map.remove(key).is_some() {
            self.write_all(&map)?;
        }
        Ok(())
    }

    fn backend(&self) -> SecretBackend {
        match self.key_source {
            KeySource::Keyring => SecretBackend::Keyring,
            KeySource::File(_) => SecretBackend::File,
        }
    }
}

/// In-memory store for tests.
#[derive(Default)]
pub struct MemorySecretStore {
    inner: Mutex<BTreeMap<String, String>>,
}

impl SecretStore for MemorySecretStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.inner.lock().unwrap().get(key).cloned())
    }
    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.inner.lock().unwrap().insert(key.into(), value.into());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<()> {
        self.inner.lock().unwrap().remove(key);
        Ok(())
    }
    fn backend(&self) -> SecretBackend {
        SecretBackend::File
    }
}

// ----------------------------------------------------------------- helpers

pub fn credential_key(base_url: &str) -> String {
    format!("jira-credential:{}", base_url.trim_end_matches('/'))
}

pub fn tempo_token_key(base_url: &str) -> String {
    format!("tempo-token:{}", base_url.trim_end_matches('/'))
}

pub fn load_credential(store: &dyn SecretStore, base_url: &str) -> Result<Option<Credential>> {
    match store.get(&credential_key(base_url))? {
        Some(raw) => Ok(Some(serde_json::from_str(&raw)?)),
        None => Ok(None),
    }
}

pub fn save_credential(
    store: &dyn SecretStore,
    base_url: &str,
    credential: &Credential,
) -> Result<()> {
    store.set(
        &credential_key(base_url),
        &serde_json::to_string(credential)?,
    )
}

pub fn delete_credential(store: &dyn SecretStore, base_url: &str) -> Result<()> {
    store.delete(&credential_key(base_url))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::StoredCookie;

    fn big_cookie(n: usize) -> StoredCookie {
        StoredCookie {
            name: format!("cloud.session.token{n}"),
            value: "x".repeat(1500),
            domain: Some("example.atlassian.net".into()),
            path: Some("/".into()),
            expires: None,
            secure: true,
            http_only: true,
        }
    }

    #[test]
    fn encrypted_roundtrip_with_large_payload() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::with_root(dir.path());
        let store = EncryptedSecretStore::file_only(&paths);
        let cred = Credential::Cookies {
            cookies: vec![big_cookie(1), big_cookie(2), big_cookie(3)],
        };
        save_credential(&store, "https://example.atlassian.net/", &cred).unwrap();
        let loaded = load_credential(&store, "https://example.atlassian.net")
            .unwrap()
            .unwrap();
        assert_eq!(loaded, cred);
        assert_eq!(store.backend(), SecretBackend::File);

        // ciphertext on disk must not contain the plaintext
        let raw = std::fs::read_to_string(paths.config_dir.join("secrets.enc")).unwrap();
        assert!(!raw.contains("cloud.session.token"));

        // a fresh store instance with the same key file can read it back
        let store2 = EncryptedSecretStore::file_only(&paths);
        assert_eq!(
            load_credential(&store2, "https://example.atlassian.net")
                .unwrap()
                .unwrap(),
            cred
        );

        delete_credential(&store2, "https://example.atlassian.net").unwrap();
        assert!(load_credential(&store2, "https://example.atlassian.net")
            .unwrap()
            .is_none());
    }

    #[test]
    fn wrong_key_fails_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::with_root(dir.path());
        let store = EncryptedSecretStore::file_only(&paths);
        store.set("a", "b").unwrap();
        std::fs::write(
            paths.config_dir.join("secrets.key"),
            b64().encode([7u8; 32]),
        )
        .unwrap();
        let store2 = EncryptedSecretStore::file_only(&paths);
        assert!(store2.get("a").is_err());
    }
}
