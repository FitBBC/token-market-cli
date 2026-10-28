//! Opt-in OS-store validation. Ordinary test runs must never write OS credentials.
#![cfg(any(target_os = "macos", target_os = "windows"))]

use anyhow::{bail, ensure, Context, Result};
use rand::RngCore;
use std::process::Command;
use tokensmarket_core::desktop::{DesktopCredentialStore, NativeDesktopCredentials};

const TEST_NAME: &str = "native_credential_round_trip_survives_process_restart";
const CHILD_SCOPE: &str = "TOKEN_MARKET_NATIVE_CREDENTIAL_SMOKE_CHILD_SCOPE";
const SCOPE_PREFIX: &str = "https://native-credential-smoke-";

fn fixture(scope: &str) -> Result<String> {
    let nonce = scope
        .strip_prefix(SCOPE_PREFIX)
        .and_then(|value| value.strip_suffix(".invalid"))
        .context("native smoke requires its own isolated .invalid namespace")?;
    ensure!(
        nonce.len() == 32 && nonce.bytes().all(|value| value.is_ascii_hexdigit()),
        "invalid native smoke fixture namespace"
    );
    Ok(serde_json::json!({
        "session_token": format!("native-credential-smoke-fixture-v1:{nonce}"),
        "expires_at": "2000-01-01T00:00:00Z",
        "user": {"id": "native-smoke-user", "name": "Native fixture"},
        "default_workspace": {"id": "native-smoke-workspace", "name": "Native fixture"},
        "web_origin": scope,
    })
    .to_string())
}

fn cleanup(store: &NativeDesktopCredentials, expected: &str) -> Result<()> {
    match store.read().context("native smoke cleanup read failed")? {
        None => return Ok(()),
        Some(value) if value == expected => {}
        Some(_) => bail!("native smoke item changed; cleanup refused to delete it"),
    }
    store
        .delete()
        .context("native smoke cleanup delete failed")?;
    ensure!(
        store.read()?.is_none(),
        "native smoke fixture was not removed"
    );
    println!("native private-store fixture delete: passed");
    Ok(())
}

fn round_trip(scope: &str) -> Result<()> {
    let expected = fixture(scope)?;
    let store = NativeDesktopCredentials::new(scope);
    ensure!(
        store.read()?.is_none(),
        "native smoke namespace already occupied; it was not changed"
    );

    // Cleanup runs even if writing or the subprocess fails. It never deletes a
    // value other than this run's fixture, and does not alter OS store settings.
    let result = store.write(&expected).and_then(|()| {
        println!("native private-store fixture write: passed");
        let child = Command::new(std::env::current_exe()?)
            .args(["--exact", TEST_NAME, "--ignored", "--nocapture"])
            .env(CHILD_SCOPE, scope)
            .status()
            .context("cannot start native smoke read process")?;
        ensure!(child.success(), "separate-process native store read failed");
        Ok(())
    });
    let cleanup_result = cleanup(&store, &expected);
    match (result, cleanup_result) {
        (Err(_), Err(_)) => {
            bail!("native smoke and fixture cleanup failed; inspect OS-store access")
        }
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

#[test]
#[ignore = "writes a unique fake fixture to the real macOS/Windows OS credential store; opt in explicitly"]
fn native_credential_round_trip_survives_process_restart() -> Result<()> {
    if let Some(scope) = std::env::var_os(CHILD_SCOPE) {
        let scope = scope.to_str().context("invalid native smoke scope")?;
        let expected = fixture(scope)?;
        let saved = NativeDesktopCredentials::new(scope).read()?;
        ensure!(
            saved.as_deref() == Some(expected.as_str()),
            "separate-process native credential fixture mismatch"
        );
        println!("native private-store fixture separate-process read: passed");
        return Ok(());
    }

    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let nonce: String = bytes.iter().map(|value| format!("{value:02x}")).collect();
    round_trip(&format!("{SCOPE_PREFIX}{nonce}.invalid"))
}
