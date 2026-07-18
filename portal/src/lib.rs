use std::{
    collections::BTreeMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub mod auth;
pub mod management;

use aes::Aes128;
use base64::{engine::general_purpose::STANDARD, Engine};
use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type Aes128CbcEnc = cbc::Encryptor<Aes128>;
type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RdpPolicy {
    #[serde(default)]
    pub clipboard_to_browser: bool,
    #[serde(default)]
    pub clipboard_to_remote: bool,
    #[serde(default)]
    pub upload: bool,
    #[serde(default)]
    pub download: bool,
    #[serde(default)]
    pub printing: bool,
    #[serde(default)]
    pub audio_output: bool,
    #[serde(default)]
    pub microphone: bool,
    #[serde(default)]
    pub recording: bool,
    #[serde(default = "default_duration")]
    pub maximum_duration_seconds: u64,
}

const fn default_duration() -> u64 {
    900
}

impl Default for RdpPolicy {
    fn default() -> Self {
        Self {
            clipboard_to_browser: false,
            clipboard_to_remote: false,
            upload: false,
            download: false,
            printing: false,
            audio_output: false,
            microphone: false,
            recording: false,
            maximum_duration_seconds: default_duration(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RdpTarget {
    pub id: String,
    pub name: String,
    pub hostname: String,
    #[serde(default = "default_rdp_port")]
    pub port: u16,
    pub certificate_fingerprint: String,
    #[serde(default)]
    pub domain: String,
}

pub struct RdpCredentials {
    pub username: String,
    pub password: String,
}

const fn default_rdp_port() -> u16 {
    3389
}

#[derive(Debug, Serialize)]
struct JsonConnection<'a> {
    protocol: &'static str,
    parameters: BTreeMap<&'static str, String>,
    id: &'a str,
}

#[derive(Debug, Serialize)]
struct JsonAuthentication<'a> {
    username: &'a str,
    expires: u128,
    connections: BTreeMap<&'a str, JsonConnection<'a>>,
}

pub fn validate_target(target: &RdpTarget) -> Result<(), String> {
    if target.id.is_empty() || target.name.is_empty() {
        return Err("target id and name are required".into());
    }
    if target.hostname.is_empty()
        || target.hostname.chars().any(char::is_whitespace)
        || target.hostname.starts_with('-')
    {
        return Err("target hostname is invalid".into());
    }
    if target.port != 3389 {
        return Err("only the policy-approved RDP port 3389 is allowed".into());
    }
    let fingerprint = target.certificate_fingerprint.replace(':', "");
    if fingerprint.len() != 64 || !fingerprint.chars().all(|value| value.is_ascii_hexdigit()) {
        return Err("a SHA-256 RDP certificate fingerprint is required".into());
    }
    Ok(())
}

pub fn guacamole_parameters(
    target: &RdpTarget,
    policy: &RdpPolicy,
    session_id: &str,
) -> Result<BTreeMap<&'static str, String>, String> {
    guacamole_parameters_with_credentials(target, policy, session_id, None)
}

pub fn guacamole_parameters_with_credentials(
    target: &RdpTarget,
    policy: &RdpPolicy,
    session_id: &str,
    credentials: Option<&RdpCredentials>,
) -> Result<BTreeMap<&'static str, String>, String> {
    validate_target(target)?;
    if policy.maximum_duration_seconds == 0 || policy.maximum_duration_seconds > 28_800 {
        return Err("session duration must be between 1 and 28800 seconds".into());
    }
    if session_id.is_empty()
        || session_id
            .chars()
            .any(|value| !value.is_ascii_alphanumeric() && value != '-')
    {
        return Err("session id is invalid".into());
    }

    let mut parameters = BTreeMap::new();
    parameters.insert("hostname", target.hostname.clone());
    parameters.insert("port", target.port.to_string());
    parameters.insert("domain", target.domain.clone());
    parameters.insert("security", "nla".into());
    parameters.insert("ignore-cert", "false".into());
    parameters.insert(
        "cert-fingerprints",
        format!(
            "sha256:{}",
            target
                .certificate_fingerprint
                .replace(':', "")
                .as_bytes()
                .chunks(2)
                .map(|pair| std::str::from_utf8(pair).expect("validated ASCII fingerprint"))
                .collect::<Vec<_>>()
                .join(":")
                .to_uppercase()
        ),
    );
    parameters.insert("disable-copy", (!policy.clipboard_to_browser).to_string());
    parameters.insert("disable-paste", (!policy.clipboard_to_remote).to_string());
    parameters.insert(
        "enable-drive",
        (policy.upload || policy.download).to_string(),
    );
    parameters.insert("disable-upload", (!policy.upload).to_string());
    parameters.insert("disable-download", (!policy.download).to_string());
    parameters.insert("enable-printing", policy.printing.to_string());
    parameters.insert("disable-audio", (!policy.audio_output).to_string());
    parameters.insert("enable-audio-input", policy.microphone.to_string());
    parameters.insert("recording-include-keys", "false".into());
    if let Some(credentials) = credentials {
        if credentials.username.is_empty()
            || credentials.username.len() > 128
            || credentials.username.chars().any(char::is_control)
            || credentials.password.is_empty()
            || credentials.password.len() > 256
            || credentials.password.chars().any(char::is_control)
        {
            return Err("RDP credentials are invalid".into());
        }
        parameters.insert("username", credentials.username.clone());
        parameters.insert("password", credentials.password.clone());
    }
    if policy.recording {
        parameters.insert("recording-path", format!("/recordings/{session_id}"));
        parameters.insert("recording-name", "session".into());
        parameters.insert("create-recording-path", "true".into());
    }
    Ok(parameters)
}

pub fn encrypted_json_auth(
    secret: &[u8; 16],
    username: &str,
    target: &RdpTarget,
    policy: &RdpPolicy,
    session_id: &str,
    lifetime: Duration,
) -> Result<String, String> {
    encrypted_json_auth_with_credentials(
        secret, username, target, policy, session_id, lifetime, None,
    )
}

pub fn encrypted_json_auth_with_credentials(
    secret: &[u8; 16],
    username: &str,
    target: &RdpTarget,
    policy: &RdpPolicy,
    session_id: &str,
    lifetime: Duration,
    credentials: Option<&RdpCredentials>,
) -> Result<String, String> {
    if username.is_empty() || username.len() > 128 || username.chars().any(char::is_control) {
        return Err("username is invalid".into());
    }
    if lifetime.is_zero() || lifetime > Duration::from_secs(60) {
        return Err("launch assertion lifetime must be between 1 and 60 seconds".into());
    }
    let expires = SystemTime::now()
        .checked_add(lifetime)
        .ok_or_else(|| "launch assertion expiration overflow".to_string())?
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("clock error: {error}"))?
        .as_millis();
    let connection = JsonConnection {
        protocol: "rdp",
        parameters: guacamole_parameters_with_credentials(target, policy, session_id, credentials)?,
        id: session_id,
    };
    let mut connections = BTreeMap::new();
    connections.insert(target.name.as_str(), connection);
    let plaintext = serde_json::to_vec(&JsonAuthentication {
        username,
        expires,
        connections,
    })
    .map_err(|error| format!("failed to encode launch assertion: {error}"))?;

    let mut mac = HmacSha256::new_from_slice(secret)
        .map_err(|_| "failed to initialize launch assertion HMAC".to_string())?;
    mac.update(&plaintext);
    let signature = mac.finalize().into_bytes();
    let mut signed = Vec::with_capacity(signature.len() + plaintext.len());
    signed.extend_from_slice(&signature);
    signed.extend_from_slice(&plaintext);
    let encrypted = Aes128CbcEnc::new(secret.into(), (&[0u8; 16]).into())
        .encrypt_padded_vec_mut::<Pkcs7>(&signed);
    Ok(STANDARD.encode(encrypted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbc::cipher::{BlockDecryptMut, KeyIvInit};

    type Aes128CbcDec = cbc::Decryptor<Aes128>;

    fn target() -> RdpTarget {
        RdpTarget {
            id: "windows-lab".into(),
            name: "Windows Lab".into(),
            hostname: "10.20.30.40".into(),
            port: 3389,
            certificate_fingerprint: "11".repeat(32),
            domain: "LAB".into(),
        }
    }

    #[test]
    fn policy_defaults_deny_every_redirection() {
        let parameters = guacamole_parameters(&target(), &RdpPolicy::default(), "session").unwrap();
        assert_eq!(parameters["disable-copy"], "true");
        assert_eq!(parameters["disable-paste"], "true");
        assert_eq!(parameters["enable-drive"], "false");
        assert_eq!(parameters["enable-printing"], "false");
        assert_eq!(parameters["disable-audio"], "true");
        assert_eq!(parameters["enable-audio-input"], "false");
        assert!(!parameters.contains_key("recording-path"));
        assert_eq!(parameters["security"], "nla");
        assert_eq!(parameters["ignore-cert"], "false");
        assert_eq!(
            parameters["cert-fingerprints"],
            format!("sha256:{}", vec!["11"; 32].join(":"))
        );
    }

    #[test]
    fn directional_controls_are_independent() {
        let policy = RdpPolicy {
            clipboard_to_remote: true,
            download: true,
            recording: true,
            ..RdpPolicy::default()
        };
        let parameters = guacamole_parameters(&target(), &policy, "abc").unwrap();
        assert_eq!(parameters["disable-copy"], "true");
        assert_eq!(parameters["disable-paste"], "false");
        assert_eq!(parameters["enable-drive"], "true");
        assert_eq!(parameters["disable-upload"], "true");
        assert_eq!(parameters["disable-download"], "false");
        assert_eq!(parameters["recording-path"], "/recordings/abc");
    }

    #[test]
    fn credentials_are_only_added_when_explicitly_supplied() {
        let credentials = RdpCredentials {
            username: "Administrator".into(),
            password: "temporary-secret".into(),
        };
        let parameters = guacamole_parameters_with_credentials(
            &target(),
            &RdpPolicy::default(),
            "session",
            Some(&credentials),
        )
        .unwrap();
        assert_eq!(parameters["username"], "Administrator");
        assert_eq!(parameters["password"], "temporary-secret");
        assert!(
            !guacamole_parameters(&target(), &RdpPolicy::default(), "session")
                .unwrap()
                .contains_key("password")
        );
    }

    #[test]
    fn invalid_targets_and_long_assertions_fail_closed() {
        let mut invalid = target();
        invalid.port = 22;
        assert!(guacamole_parameters(&invalid, &RdpPolicy::default(), "x").is_err());
        assert!(encrypted_json_auth(
            &[7; 16],
            "user",
            &target(),
            &RdpPolicy::default(),
            "x",
            Duration::from_secs(61)
        )
        .is_err());
    }

    #[test]
    fn encrypted_assertion_is_block_aligned() {
        let secret = [7; 16];
        let value = encrypted_json_auth(
            &secret,
            "user",
            &target(),
            &RdpPolicy::default(),
            "session",
            Duration::from_secs(30),
        )
        .unwrap();
        assert!(!value.is_empty());
        let encrypted = STANDARD.decode(value).unwrap();
        assert_eq!(encrypted.len() % 16, 0);
        let signed = Aes128CbcDec::new((&secret).into(), (&[0u8; 16]).into())
            .decrypt_padded_vec_mut::<Pkcs7>(&encrypted)
            .unwrap();
        let (signature, plaintext) = signed.split_at(32);
        let mut mac = HmacSha256::new_from_slice(&secret).unwrap();
        mac.update(plaintext);
        mac.verify_slice(signature).unwrap();
        let payload: serde_json::Value = serde_json::from_slice(plaintext).unwrap();
        assert_eq!(payload["username"], "user");
        assert_eq!(payload["connections"]["Windows Lab"]["protocol"], "rdp");
        assert_eq!(
            payload["connections"]["Windows Lab"]["parameters"]["disable-copy"],
            "true"
        );
    }
}
