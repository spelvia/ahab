//! The machine key: signing with it, and checking signatures. No I/O here.

use std::fmt;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

use crate::modules::machines::machine::model::PublicKey;

/// A machine's Ed25519 key pair.
pub struct MachineKey(SigningKey);

impl MachineKey {
    pub fn from_secret(secret: [u8; 32]) -> Self {
        Self(SigningKey::from_bytes(&secret))
    }

    /// The private half, to store it.
    pub fn secret(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    pub fn public_key(&self) -> PublicKey {
        // A verifying key always has 32 bytes.
        PublicKey::from_bytes(&self.0.verifying_key().to_bytes())
            .unwrap_or_else(|_| unreachable!("an Ed25519 public key has 32 bytes"))
    }

    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.0.sign(message).to_bytes()
    }
}

impl fmt::Debug for MachineKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("MachineKey")
            .field(&self.public_key().fingerprint().to_string())
            .finish()
    }
}

/// Whether `signature` is `public_key`'s signature of `message`.
pub fn verify(public_key: &PublicKey, message: &[u8], signature: &[u8]) -> bool {
    let Ok(key) = VerifyingKey::from_bytes(public_key.as_bytes()) else {
        return false;
    };
    let Ok(signature) = Signature::try_from(signature) else {
        return false;
    };
    key.verify_strict(message, &signature).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_what_only_its_public_key_verifies() {
        let key = MachineKey::from_secret([3; 32]);
        let other = MachineKey::from_secret([4; 32]);
        let signature = key.sign(b"hello");

        assert!(verify(&key.public_key(), b"hello", &signature));
        assert!(!verify(&key.public_key(), b"hullo", &signature));
        assert!(!verify(&other.public_key(), b"hello", &signature));
        assert!(!verify(&key.public_key(), b"hello", &signature[..63]));
    }

    #[test]
    fn keeps_its_secret_out_of_debug_output() {
        let key = MachineKey::from_secret([3; 32]);
        let text = format!("{key:?}");
        assert!(text.starts_with("MachineKey(\"SHA256:"));
    }
}
