//! Shared management-session entry points for the CLI and desktop application.
use crate::{
    config,
    desktop::{DesktopClient, DesktopLogin, DesktopSession, NativeDesktopCredentials},
};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub fn login_directory(origin: &str) -> std::path::PathBuf {
    config::token_market_home()
        .join("client-login")
        .join(format!(
            "{:x}",
            Sha256::digest(origin.trim_end_matches('/').as_bytes())
        ))
}

pub fn login_manager() -> DesktopLogin {
    let origin = config::console_api_base_url();
    DesktopLogin::with_credentials(
        login_directory(&origin),
        DesktopClient::new(origin.clone()),
        Arc::new(NativeDesktopCredentials::new(&origin)),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn pending_login_and_locks_are_shared_per_environment() {
        assert_eq!(
            super::login_directory("https://console.example"),
            super::login_directory("https://console.example/")
        );
        assert_ne!(
            super::login_directory("https://console.example"),
            super::login_directory("https://console-test.example")
        );
    }
}
pub async fn require_session() -> Result<DesktopSession> {
    login_manager()
        .restore()
        .await?
        .context("请先运行 tokenmarket login；CLI 和桌面共享登录")
}
