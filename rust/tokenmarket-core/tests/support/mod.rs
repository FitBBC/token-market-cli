#![allow(dead_code)]
mod client_protocol;
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD, Engine};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
use tokenmarket_core::{connections::ConnectionManager, desktop::DesktopCredentialStore};
#[derive(Default)]
pub struct Credentials(pub Mutex<Option<String>>);
impl DesktopCredentialStore for Credentials {
    fn read(&self) -> Result<Option<String>> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn write(&self, value: &str) -> Result<()> {
        *self.0.lock().unwrap() = Some(value.into());
        Ok(())
    }
    fn delete(&self) -> Result<()> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}
pub fn credentials(directory: &Path) -> Arc<Credentials> {
    static STORES: OnceLock<Mutex<HashMap<PathBuf, Arc<Credentials>>>> = OnceLock::new();
    STORES
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .entry(directory.into())
        .or_default()
        .clone()
}
pub fn manager(directory: PathBuf, platform: String, gateway: String) -> ConnectionManager {
    let platform = client_protocol::bridge(&directory, platform);
    ConnectionManager::with_credentials(
        directory.clone(),
        platform,
        gateway,
        credentials(&directory),
    )
}
pub fn read_private(path: &Path) -> Vec<u8> {
    let bytes = std::fs::read(path).unwrap();
    let magic = b"TOKENMARKET-PRIVATE-1\n";
    if !bytes.starts_with(magic) {
        return bytes;
    }
    let parent = path.parent().unwrap();
    let directory = if parent.file_name().unwrap() == "backups" {
        parent.parent().unwrap()
    } else {
        parent
    };
    let store = credentials(directory);
    let master = store.read().unwrap().unwrap();
    let key =
        LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &STANDARD.decode(master).unwrap()).unwrap());
    let payload = &bytes[magic.len()..];
    let mut encrypted = payload[12..].to_vec();
    key.open_in_place(
        Nonce::assume_unique_for_key(payload[..12].try_into().unwrap()),
        Aad::from(path.to_string_lossy().as_bytes()),
        &mut encrypted,
    )
    .unwrap()
    .to_vec()
}
