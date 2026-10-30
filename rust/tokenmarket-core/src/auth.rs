use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct Pkce {
    pub code_verifier: String,
    pub code_challenge: String,
    pub code_challenge_method: &'static str,
}

pub fn create_pkce() -> Pkce {
    let mut bytes = [0u8; 48];
    rand::thread_rng().fill_bytes(&mut bytes);
    let code_verifier = URL_SAFE_NO_PAD.encode(bytes);
    let digest = Sha256::digest(code_verifier.as_bytes());
    Pkce {
        code_verifier,
        code_challenge: URL_SAFE_NO_PAD.encode(digest),
        code_challenge_method: "S256",
    }
}

pub fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let command = "open";
    #[cfg(target_os = "windows")]
    let command = "cmd";
    #[cfg(all(unix, not(target_os = "macos")))]
    let command = "xdg-open";

    #[cfg(target_os = "windows")]
    let mut process = std::process::Command::new(command)
        .args(["/C", "start", "", url])
        .spawn()
        .context("open browser")?;
    #[cfg(not(target_os = "windows"))]
    let mut process = std::process::Command::new(command)
        .arg(url)
        .spawn()
        .context("open browser")?;
    let _ = process.wait();
    Ok(())
}
