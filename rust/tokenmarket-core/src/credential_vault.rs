//! Linux headless fallback: authenticated encryption, a user passphrase, no plaintext fallback.
use crate::desktop::DesktopCredentialStore;
use anyhow::{bail, Context, Result};
use rand::{rngs::OsRng, RngCore};
use ring::{
    aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM},
    pbkdf2,
};
use std::{fs, num::NonZeroU32, path::PathBuf};

const HEADER: &[u8] = b"TM-PASSPHRASE-VAULT-1\n";
const ITERATIONS: u32 = 600_000;

pub struct PassphraseVault {
    path: PathBuf,
    passphrase: Vec<u8>,
}
impl Drop for PassphraseVault {
    fn drop(&mut self) {
        self.passphrase.fill(0);
    }
}
impl PassphraseVault {
    pub fn new(path: PathBuf, passphrase: Vec<u8>) -> Result<Self> {
        if passphrase.len() < 12 {
            bail!("凭证库口令至少需要 12 个字节")
        }
        Ok(Self { path, passphrase })
    }
    fn key(&self, salt: &[u8]) -> Result<LessSafeKey> {
        let mut key = [0; 32];
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA256,
            NonZeroU32::new(ITERATIONS).unwrap(),
            salt,
            &self.passphrase,
            &mut key,
        );
        let result = LessSafeKey::new(
            UnboundKey::new(&AES_256_GCM, &key).map_err(|_| anyhow::anyhow!("无法解锁凭证库"))?,
        );
        key.fill(0);
        Ok(result)
    }
    fn check_path(&self) -> Result<()> {
        if fs::symlink_metadata(&self.path).is_ok_and(|meta| meta.file_type().is_symlink()) {
            bail!("凭证库是符号链接，请检查实际位置")
        }
        Ok(())
    }
}
impl DesktopCredentialStore for PassphraseVault {
    fn read(&self) -> Result<Option<String>> {
        self.check_path()?;
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => bail!("无法读取凭证库"),
        };
        if !bytes.starts_with(HEADER) || bytes.len() < HEADER.len() + 44 {
            bail!("凭证库格式无效，原文件已保留")
        }
        let payload = &bytes[HEADER.len()..];
        let key = self.key(&payload[..16])?;
        let nonce: [u8; 12] = payload[16..28].try_into().unwrap();
        let mut encrypted = payload[28..].to_vec();
        let plain = key
            .open_in_place(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(self.path.to_string_lossy().as_bytes()),
                &mut encrypted,
            )
            .map_err(|_| anyhow::anyhow!("凭证库口令错误或文件损坏，原文件已保留"))?;
        Ok(Some(
            String::from_utf8(plain.to_vec()).context("凭证库内容无效")?,
        ))
    }
    fn write(&self, value: &str) -> Result<()> {
        self.check_path()?;
        // Do not overwrite an existing vault until this passphrase has authenticated it.
        self.read()?;
        let mut salt = [0; 16];
        let mut nonce = [0; 12];
        OsRng.fill_bytes(&mut salt);
        OsRng.fill_bytes(&mut nonce);
        let key = self.key(&salt)?;
        let mut encrypted = value.as_bytes().to_vec();
        key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(self.path.to_string_lossy().as_bytes()),
            &mut encrypted,
        )
        .map_err(|_| anyhow::anyhow!("无法加密凭证库"))?;
        let mut bytes = HEADER.to_vec();
        bytes.extend(salt);
        bytes.extend(nonce);
        bytes.extend(encrypted);
        crate::connections::atomic_write(&self.path, &bytes)
    }
    fn delete(&self) -> Result<()> {
        self.check_path()?;
        self.read()?;
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => bail!("无法删除登录凭证"),
        }
    }
}

#[cfg(any(target_os = "linux", test))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) mod linux {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::{
        io::Write,
        process::{Command, Stdio},
        sync::{Mutex, OnceLock},
    };
    pub fn fallback_path(service: &str, account: &str) -> PathBuf {
        crate::config::token_market_home()
            .join("secure-store")
            .join(format!(
                "{:x}.vault",
                Sha256::digest(format!("{service}\n{account}").as_bytes())
            ))
    }
    fn password() -> Result<Vec<u8>> {
        static PASSWORD: OnceLock<Mutex<Option<Vec<u8>>>> = OnceLock::new();
        let mut cached = PASSWORD
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| anyhow::anyhow!("凭证库已锁定"))?;
        if let Some(password) = cached.as_ref() {
            return Ok(password.clone());
        }
        let mut password = if let Ok(fd) = std::env::var("TOKEN_MARKET_VAULT_PASSWORD_FD") {
            let fd: u32 = fd.parse().context("口令描述符无效")?;
            if fd < 3 {
                bail!("口令需要通过独立文件描述符传入")
            }
            fs::read(format!("/proc/self/fd/{fd}")).context("无法读取口令描述符")?
        } else {
            let output = Command::new("systemd-ask-password").arg("Token Market credential vault passphrase (minimum 12 bytes):").output()
                .context("无法安全询问口令；请安装 systemd-ask-password 或通过 TOKEN_MARKET_VAULT_PASSWORD_FD 提供口令")?;
            if !output.status.success() {
                bail!("凭证库未解锁")
            }
            output.stdout
        };
        while password
            .last()
            .is_some_and(|byte| matches!(byte, b'\r' | b'\n'))
        {
            password.pop();
        }
        if password.len() < 12 {
            bail!("凭证库口令至少需要 12 个字节")
        }
        *cached = Some(password.clone());
        Ok(password)
    }
    fn fallback(service: &str, account: &str) -> Result<PassphraseVault> {
        PassphraseVault::new(fallback_path(service, account), password()?)
    }
    fn secret_tool_available() -> bool {
        std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some()
            && Command::new("secret-tool")
                .arg("--version")
                .output()
                .is_ok()
    }
    pub fn read(service: &str, account: &str) -> Result<Option<String>> {
        if fallback_path(service, account).exists() || !secret_tool_available() {
            return fallback(service, account)?.read();
        }
        let output = Command::new("secret-tool")
            .args(["lookup", "service", service, "account", account])
            .output()
            .context("无法访问系统密钥环")?;
        if !output.status.success() {
            if output.status.code() == Some(1)
                && output.stdout.is_empty()
                && output.stderr.is_empty()
            {
                return Ok(None);
            }
            bail!("系统密钥环不可读，请先解锁；未切换到其他凭证库")
        }
        Ok(Some(
            String::from_utf8(output.stdout)
                .context("系统凭证无效")?
                .trim_end_matches(['\r', '\n'])
                .to_owned(),
        ))
    }
    pub fn write(service: &str, account: &str, value: &str) -> Result<()> {
        if fallback_path(service, account).exists() || !secret_tool_available() {
            return fallback(service, account)?.write(value);
        }
        let mut child = Command::new("secret-tool")
            .args([
                "store",
                "--label=Token Market",
                "service",
                service,
                "account",
                account,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("无法访问系统密钥环")?;
        child
            .stdin
            .take()
            .context("无法保存系统凭证")?
            .write_all(value.as_bytes())?;
        if !child.wait()?.success() {
            bail!("无法保存系统凭证，请解锁密钥环")
        }
        Ok(())
    }
    pub fn delete(service: &str, account: &str) -> Result<()> {
        if fallback_path(service, account).exists() || !secret_tool_available() {
            return fallback(service, account)?.delete();
        }
        let status = Command::new("secret-tool")
            .args(["clear", "service", service, "account", account])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !status.success() {
            bail!("无法删除系统登录凭证")
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_vault_rejects_wrong_password_tampering_and_relocation_without_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault");
        let vault =
            PassphraseVault::new(path.clone(), b"a long fixture passphrase".to_vec()).unwrap();
        vault.write("fixture-private-session").unwrap();
        let original = fs::read(&path).unwrap();
        assert!(!original
            .windows(23)
            .any(|bytes| bytes == b"fixture-private-session"));
        assert_eq!(
            vault.read().unwrap().as_deref(),
            Some("fixture-private-session")
        );
        let wrong =
            PassphraseVault::new(path.clone(), b"another fixture passphrase".to_vec()).unwrap();
        assert!(wrong.read().is_err());
        assert!(wrong.write("replacement").is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        let moved = dir.path().join("moved");
        fs::write(&moved, &original).unwrap();
        assert!(
            PassphraseVault::new(moved, b"a long fixture passphrase".to_vec())
                .unwrap()
                .read()
                .is_err()
        );
        let mut modified = original;
        *modified.last_mut().unwrap() ^= 1;
        fs::write(&path, modified).unwrap();
        assert!(vault.read().is_err());
    }
}
