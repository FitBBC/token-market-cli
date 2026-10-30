//! App-owned secrets are encrypted at rest; only the encryption key lives in the OS vault.
//! Tool configuration must still contain the credential required by the third-party tool.
use super::atomic_write;
use crate::desktop::{CachedDesktopCredentials, DesktopCredentialStore, NativeDesktopCredentials};
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use rand::{rngs::OsRng, RngCore};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use std::{fs, path::Path, sync::Arc};

const MAGIC: &[u8] = b"TOKENMARKET-PRIVATE-1\n";

pub(super) struct PrivateStorage {
    credentials: Arc<dyn DesktopCredentialStore>,
}
impl PrivateStorage {
    pub fn native(directory: &Path) -> Self {
        Self::new(Arc::new(CachedDesktopCredentials::new(Arc::new(
            NativeDesktopCredentials::scoped(
                "cn.tokenmarket.setup.tool-credentials",
                &directory.to_string_lossy(),
            ),
        ))))
    }
    pub fn new(credentials: Arc<dyn DesktopCredentialStore>) -> Self {
        Self { credentials }
    }
    fn key(&self, create: bool) -> Result<LessSafeKey> {
        let encoded = match self.credentials.read()? {
            Some(value) => value,
            None if create => {
                let mut bytes = [0; 32];
                OsRng.fill_bytes(&mut bytes);
                let value = STANDARD.encode(bytes);
                self.credentials.write(&value)?;
                value
            }
            None => bail!("系统私密存储中的加密凭据已丢失，无法读取原连接，请先恢复私密存储"),
        };
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| anyhow::anyhow!("本地凭据存储无效"))?;
        Ok(LessSafeKey::new(
            UnboundKey::new(&AES_256_GCM, &bytes)
                .map_err(|_| anyhow::anyhow!("本地凭据存储无效"))?,
        ))
    }
    pub fn encode(&self, path: &Path, bytes: &[u8]) -> Result<Vec<u8>> {
        let key = self.key(true)?;
        let mut nonce = [0; 12];
        OsRng.fill_bytes(&mut nonce);
        let mut encrypted = bytes.to_vec();
        key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(path.to_string_lossy().as_bytes()),
            &mut encrypted,
        )
        .map_err(|_| anyhow::anyhow!("无法保护本地凭据"))?;
        let mut result = MAGIC.to_vec();
        result.extend_from_slice(&nonce);
        result.extend(encrypted);
        Ok(result)
    }
    pub fn read(&self, path: &Path) -> Result<Vec<u8>> {
        if fs::symlink_metadata(path)?.file_type().is_symlink() {
            bail!("凭据文件是符号链接，请先检查本地配置")
        }
        let bytes = fs::read(path)?;
        self.decode(path, &bytes)
    }
    pub fn decode(&self, path: &Path, bytes: &[u8]) -> Result<Vec<u8>> {
        if !bytes.starts_with(MAGIC) {
            // Legacy files are upgraded only after their contents have been parsed successfully.
            return Ok(bytes.to_vec());
        }
        let payload = &bytes[MAGIC.len()..];
        if payload.len() < 28 {
            bail!("本地凭据文件损坏")
        }
        let nonce: [u8; 12] = payload[..12].try_into().unwrap();
        let mut encrypted = payload[12..].to_vec();
        let key = self.key(false)?;
        let plain = key
            .open_in_place(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(path.to_string_lossy().as_bytes()),
                &mut encrypted,
            )
            .map_err(|_| anyhow::anyhow!("无法解密本地凭据，请检查系统私密存储"))?;
        Ok(plain.to_vec())
    }
    pub fn write(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        atomic_write(path, &self.encode(path, bytes)?)
    }
    pub fn protect_legacy(&self, path: &Path) -> Result<()> {
        if fs::symlink_metadata(path)?.file_type().is_symlink() {
            bail!("凭据文件是符号链接，请先检查本地配置")
        }
        let bytes = fs::read(path).context("无法读取旧连接备份")?;
        if !bytes.starts_with(MAGIC) {
            self.write(path, &bytes)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    #[derive(Default)]
    struct Credentials {
        value: Mutex<Option<String>>,
        deny: bool,
    }
    impl DesktopCredentialStore for Credentials {
        fn read(&self) -> Result<Option<String>> {
            Ok(self.value.lock().unwrap().clone())
        }
        fn write(&self, value: &str) -> Result<()> {
            if self.deny {
                bail!("denied")
            }
            *self.value.lock().unwrap() = Some(value.into());
            Ok(())
        }
        fn delete(&self) -> Result<()> {
            *self.value.lock().unwrap() = None;
            Ok(())
        }
    }
    #[test]
    fn persisted_ciphertext_survives_restart_and_rejects_tampering_or_relocation() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("keys.json");
        let credentials = Arc::new(Credentials::default());
        let private = PrivateStorage::new(credentials.clone());
        private.write(&path, b"one-time-secret").unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(!bytes.windows(15).any(|value| value == b"one-time-secret"));
        let reopened = PrivateStorage::new(credentials.clone());
        assert_eq!(reopened.read(&path).unwrap(), b"one-time-secret");
        let mut damaged = bytes.clone();
        *damaged.last_mut().unwrap() ^= 1;
        assert!(reopened.decode(&path, &damaged).is_err());
        assert!(reopened.decode(&root.path().join("other"), &bytes).is_err());
        credentials.delete().unwrap();
        assert!(reopened.read(&path).is_err());
        assert!(
            credentials.read().unwrap().is_none(),
            "reading never replaces a missing encryption key"
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    #[test]
    fn denied_vault_storage_never_writes_a_plaintext_fallback_or_overwrites_legacy_data() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("keys.json");
        let private = PrivateStorage::new(Arc::new(Credentials {
            deny: true,
            ..Default::default()
        }));
        assert!(private.write(&path, b"secret").is_err());
        assert!(!path.exists());
        fs::write(&path, b"legacy-secret").unwrap();
        assert!(private.protect_legacy(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"legacy-secret");
    }
}
