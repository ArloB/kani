use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data).as_slice())
}

pub fn key_fingerprint(pubkey_bytes: &[u8; 32]) -> String {
    let digest = Sha256::digest(pubkey_bytes);
    format!("SHA256:{}", B64.encode(digest.as_slice()))
}

#[derive(Debug, thiserror::Error)]
pub enum SigningError {
    #[error("invalid base64: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("invalid key length (expected 32 bytes)")]
    InvalidKeyLength,
    #[error("invalid signature length (expected 64 bytes)")]
    InvalidSignatureLength,
    #[error("invalid public key: {0}")]
    InvalidPublicKey(String),
    #[error("signature verification failed")]
    VerificationFailed,
    #[error("sha256 mismatch: expected {expected}, got {actual}")]
    DigestMismatch { expected: String, actual: String },
}

pub fn pubkey_b64(signing_key: &SigningKey) -> String {
    B64.encode(signing_key.verifying_key().as_bytes())
}

pub fn sign_artifact(artifact: &[u8], signing_key: &SigningKey) -> [u8; 64] {
    use ed25519_dalek::Signer as _;
    signing_key.sign(artifact).to_bytes()
}

pub fn signature_b64(sig_bytes: &[u8; 64]) -> String {
    B64.encode(sig_bytes)
}

pub fn verify_artifact(
    artifact: &[u8],
    pubkey_b64: &str,
    sig_b64: &str,
) -> Result<(), SigningError> {
    let pubkey_bytes = B64.decode(pubkey_b64)?;
    let pubkey_arr: &[u8; 32] = pubkey_bytes
        .as_slice()
        .try_into()
        .map_err(|_| SigningError::InvalidKeyLength)?;
    let verifying_key = VerifyingKey::from_bytes(pubkey_arr)
        .map_err(|e| SigningError::InvalidPublicKey(e.to_string()))?;
    let sig_bytes = B64.decode(sig_b64)?;
    let sig_arr: &[u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| SigningError::InvalidSignatureLength)?;
    let signature = Signature::from_bytes(sig_arr);
    verifying_key
        .verify_strict(artifact, &signature)
        .map_err(|_| SigningError::VerificationFailed)
}

pub fn verify_sha256(artifact: &[u8], expected_hex: &str) -> Result<(), SigningError> {
    let actual = sha256_hex(artifact);
    if actual != expected_hex {
        return Err(SigningError::DigestMismatch {
            expected: expected_hex.to_string(),
            actual,
        });
    }
    Ok(())
}

pub fn load_signing_key(path: &std::path::Path) -> Result<SigningKey, crate::error::CliError> {
    let b64 = std::fs::read_to_string(path)?.trim().to_string();
    let seed_bytes = B64.decode(&b64).map_err(|e| {
        crate::error::CliError::Other(format!(
            "Invalid key file '{path}': {e}",
            path = path.display()
        ))
    })?;
    let seed_arr: [u8; 32] = seed_bytes.try_into().map_err(|_| {
        crate::error::CliError::Other(format!(
            "Key file '{}' must contain a 32-byte (44-char base64) seed",
            path.display()
        ))
    })?;
    Ok(SigningKey::from_bytes(&seed_arr))
}

pub fn load_verifying_key(
    path: &std::path::Path,
) -> Result<([u8; 32], String), crate::error::CliError> {
    let b64 = std::fs::read_to_string(path)?.trim().to_string();
    let key_bytes = B64.decode(&b64).map_err(|e| {
        crate::error::CliError::Other(format!("Invalid public key file '{}': {e}", path.display()))
    })?;
    let key_arr: [u8; 32] = key_bytes.try_into().map_err(|_| {
        crate::error::CliError::Other(format!(
            "Public key file '{}' must contain 32 bytes (44-char base64)",
            path.display()
        ))
    })?;
    Ok((key_arr, b64))
}
