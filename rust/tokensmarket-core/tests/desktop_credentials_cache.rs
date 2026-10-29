use anyhow::{bail, Result};
use std::sync::{Arc, Mutex};
use tokensmarket_core::desktop::{CachedDesktopCredentials, DesktopCredentialStore};

#[derive(Default)]
struct State {
    value: Option<String>,
    reads: usize,
    writes: usize,
    deletes: usize,
    fail: bool,
}
#[derive(Default)]
struct Store(Mutex<State>);
impl DesktopCredentialStore for Store {
    fn read(&self) -> Result<Option<String>> {
        let mut state = self.0.lock().unwrap();
        state.reads += 1;
        if state.fail {
            bail!("denied")
        }
        Ok(state.value.clone())
    }
    fn write(&self, value: &str) -> Result<()> {
        let mut state = self.0.lock().unwrap();
        state.writes += 1;
        if state.fail {
            bail!("denied")
        }
        state.value = Some(value.into());
        Ok(())
    }
    fn delete(&self) -> Result<()> {
        let mut state = self.0.lock().unwrap();
        state.deletes += 1;
        if state.fail {
            bail!("denied")
        }
        state.value = None;
        Ok(())
    }
}

#[test]
fn repeated_reads_and_unchanged_writes_access_the_os_once() {
    let store = Arc::new(Store::default());
    store.0.lock().unwrap().value = Some("fixture".into());
    let cached = CachedDesktopCredentials::new(store.clone());
    for _ in 0..4 {
        assert_eq!(cached.read().unwrap().as_deref(), Some("fixture"));
    }
    cached.write("fixture").unwrap();
    assert_eq!(store.0.lock().unwrap().reads, 1);
    assert_eq!(store.0.lock().unwrap().writes, 0);
    cached.write("renewed").unwrap();
    assert_eq!(cached.read().unwrap().as_deref(), Some("renewed"));
    assert_eq!(store.0.lock().unwrap().writes, 1);
    let restarted = CachedDesktopCredentials::new(store.clone());
    assert_eq!(restarted.read().unwrap().as_deref(), Some("renewed"));
    assert_eq!(store.0.lock().unwrap().reads, 2);
}

#[test]
fn logout_removes_persistence_and_cached_identity() {
    let store = Arc::new(Store::default());
    let cached = CachedDesktopCredentials::new(store.clone());
    cached.write("account-a").unwrap();
    cached.delete().unwrap();
    assert!(cached.read().unwrap().is_none());
    assert!(store.0.lock().unwrap().value.is_none());
    cached.write("account-b").unwrap();
    assert_eq!(cached.read().unwrap().as_deref(), Some("account-b"));
}

#[test]
fn denied_operations_never_create_a_successful_cache_entry() {
    let store = Arc::new(Store::default());
    store.0.lock().unwrap().fail = true;
    let cached = CachedDesktopCredentials::new(store.clone());
    assert!(cached.read().is_err());
    assert!(cached.write("fixture").is_err());
    store.0.lock().unwrap().fail = false;
    assert!(cached.read().unwrap().is_none());
    cached.write("fixture").unwrap();
    store.0.lock().unwrap().fail = true;
    assert!(cached.delete().is_err());
    assert!(cached.read().is_err());
    store.0.lock().unwrap().fail = false;
    assert_eq!(cached.read().unwrap().as_deref(), Some("fixture"));
}

#[test]
fn separate_platform_stores_do_not_share_cached_credentials() {
    let test = CachedDesktopCredentials::new(Arc::new(Store::default()));
    let production = CachedDesktopCredentials::new(Arc::new(Store::default()));
    test.write("test-only").unwrap();
    assert!(production.read().unwrap().is_none());
}
