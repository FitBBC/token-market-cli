use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use sha2::{Digest, Sha256};
use tokensmarket_core::auth::create_pkce;

#[test]
fn creates_s256_pkce_pairs() {
    let pkce = create_pkce();
    assert_eq!(pkce.code_challenge_method, "S256");
    assert!(pkce
        .code_verifier
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    let digest = Sha256::digest(pkce.code_verifier.as_bytes());
    assert_eq!(pkce.code_challenge, URL_SAFE_NO_PAD.encode(digest));
}
