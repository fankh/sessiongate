use std::time::{SystemTime, UNIX_EPOCH};

use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use hmac::{Hmac, Mac};
use rand::{rngs::OsRng, RngCore};
use sha1::Sha1;
use sha2::{Digest, Sha256};

type HmacSha1 = Hmac<Sha1>;

pub const SESSION_BYTES: usize = 32;
pub const CSRF_BYTES: usize = 32;

pub fn hash_password(password: &str) -> Result<String, String> {
    validate_password(password)?;
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| format!("password hashing failed: {error}"))
}

pub fn verify_password(encoded: &str, password: &str) -> bool {
    let Ok(hash) = PasswordHash::new(encoded) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &hash)
        .is_ok()
}

pub fn validate_password(password: &str) -> Result<(), String> {
    if password.len() < 14 || password.len() > 256 || password.chars().any(char::is_control) {
        return Err("password must contain 14-256 non-control characters".into());
    }
    Ok(())
}

pub fn random_token<const N: usize>() -> [u8; N] {
    let mut value = [0_u8; N];
    OsRng.fill_bytes(&mut value);
    value
}

pub fn token_hash(token: &[u8]) -> [u8; 32] {
    Sha256::digest(token).into()
}

pub fn verify_totp(secret: &[u8], supplied: &str, now_seconds: u64) -> bool {
    let Ok(code) = supplied.parse::<u32>() else {
        return false;
    };
    if supplied.len() != 6 {
        return false;
    }
    (-1_i64..=1).any(|offset| {
        let counter = (now_seconds / 30) as i64 + offset;
        counter >= 0 && totp(secret, counter as u64) == code
    })
}

pub fn verify_totp_now(secret: &[u8], supplied: &str) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default();
    verify_totp(secret, supplied, now)
}

pub fn mfa_satisfied(secret: &[u8], supplied: &str) -> bool {
    secret.is_empty() || verify_totp_now(secret, supplied)
}

fn totp(secret: &[u8], counter: u64) -> u32 {
    let mut mac = HmacSha1::new_from_slice(secret).expect("HMAC accepts arbitrary key sizes");
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = (digest[19] & 0x0f) as usize;
    let binary = ((u32::from(digest[offset]) & 0x7f) << 24)
        | (u32::from(digest[offset + 1]) << 16)
        | (u32::from(digest[offset + 2]) << 8)
        | u32::from(digest[offset + 3]);
    binary % 1_000_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hashes_are_salted_and_verify() {
        let first = hash_password("correct horse battery staple").unwrap();
        let second = hash_password("correct horse battery staple").unwrap();
        assert_ne!(first, second);
        assert!(verify_password(&first, "correct horse battery staple"));
        assert!(!verify_password(&first, "incorrect horse battery staple"));
    }

    #[test]
    fn totp_matches_rfc_6238_sha1_vector_and_allows_clock_skew() {
        let secret = b"12345678901234567890";
        assert!(verify_totp(secret, "287082", 59));
        assert!(verify_totp(secret, "287082", 60));
        assert!(!verify_totp(secret, "287082", 120));
    }

    #[test]
    fn tokens_are_stored_as_one_way_hashes() {
        let token = random_token::<SESSION_BYTES>();
        assert_ne!(token.as_slice(), token_hash(&token).as_slice());
    }

    #[test]
    fn mfa_is_optional_only_until_a_secret_is_enrolled() {
        assert!(mfa_satisfied(&[], ""));
        assert!(!mfa_satisfied(b"configured-totp-secret", ""));
        assert!(!mfa_satisfied(b"configured-totp-secret", "000000"));
    }
}
