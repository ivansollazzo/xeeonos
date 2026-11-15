//! Ed25519 keys: making them, signing, checking signatures.
//!
//! A secret key file holds one line: `xapp-ed25519-secret <64 hex digits>`.
//! A public key is written as 64 hex digits (it is also the manifest's
//! `signer`).

use std::fs;
use std::io::Read;
use std::path::Path;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use xapp_core::util::{Result, err, hex, unhex, write_atomic};

const SECRET_PREFIX: &str = "xapp-ed25519-secret ";

/// A secret key, loaded.
pub struct Secret(SigningKey);

impl Secret {
    /// Its public key, as hex.
    pub fn public(&self) -> String {
        hex(self.0.verifying_key().as_bytes())
    }

    /// Sign a message.
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.0.sign(message).to_bytes()
    }
}

/// Make a new key and write it to `path` (readable only by the user).
pub fn generate(path: &Path) -> Result<Secret> {
    let mut seed = [0u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut seed)?;
    let key = SigningKey::from_bytes(&seed);
    write_atomic(path, format!("{SECRET_PREFIX}{}\n", hex(&seed)).as_bytes())?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(Secret(key))
}

/// Load a secret key file.
pub fn load(path: &Path) -> Result<Secret> {
    let text = fs::read_to_string(path)
        .map_err(|e| err(format!("cannot read key {}: {e}", path.display())))?;
    parse_secret(&text).ok_or_else(|| err(format!("{} is not an xapp secret key", path.display())))
}

/// A secret key from its text (also from an environment variable in CI).
pub fn parse_secret(text: &str) -> Option<Secret> {
    let seed: [u8; 32] = unhex(text.trim().strip_prefix(SECRET_PREFIX)?)?
        .try_into()
        .ok()?;
    Some(Secret(SigningKey::from_bytes(&seed)))
}

/// Check a signature made by `public` (hex) of `message`.
pub fn verify(public: &str, message: &[u8], signature: &[u8; 64]) -> Result<()> {
    let bytes: [u8; 32] = unhex(public)
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| err("the author's key is not valid"))?;
    let key = VerifyingKey::from_bytes(&bytes).map_err(|_| err("the author's key is not valid"))?;
    key.verify_strict(message, &Signature::from_bytes(signature))
        .map_err(|_| {
            err("the signature is not valid: the file was changed or is not from its author")
        })
}

/// A short form of a public key for people: the first 16 hex digits in
/// groups of four ("3F2A 9C01 ...").
pub fn fingerprint(public: &str) -> String {
    public
        .chars()
        .take(16)
        .collect::<Vec<char>>()
        .chunks(4)
        .map(|c| c.iter().collect::<String>().to_uppercase())
        .collect::<Vec<String>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify() {
        let dir = std::env::temp_dir().join(format!("xapp-keys-test-{}", std::process::id()));
        let path = dir.join("key");
        let secret = generate(&path).unwrap();
        let again = load(&path).unwrap();
        assert_eq!(secret.public(), again.public());
        let sig = secret.sign(b"manifest");
        verify(&secret.public(), b"manifest", &sig).unwrap();
        assert!(verify(&secret.public(), b"manifesT", &sig).is_err());
        let other = generate(&dir.join("other")).unwrap();
        assert!(verify(&other.public(), b"manifest", &sig).is_err());
        assert_eq!(fingerprint(&"ab".repeat(32)), "ABAB ABAB ABAB ABAB");
        fs::remove_dir_all(&dir).unwrap();
    }
}
