//! Разбор закрытых ключей и отпечатки.

use std::path::Path;

use russh::keys::{Algorithm, HashAlg, PrivateKey, PublicKey};

use crate::error::{CoreError, CoreResult};

/// Тип ключа в стиле OpenSSH (`ssh-ed25519`, `ssh-rsa`, `ecdsa-sha2-nistp256`).
pub fn key_type(alg: &Algorithm) -> String {
    if matches!(alg, Algorithm::Rsa { .. }) {
        "ssh-rsa".to_string()
    } else {
        alg.as_str().to_string()
    }
}

pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// Расшифровать ключ из текста. Зашифрованный без фразы — `auth.secret_required`.
pub fn decode_key(text: &str, passphrase: Option<&str>) -> CoreResult<PrivateKey> {
    match russh::keys::decode_secret_key(text, None) {
        Ok(k) => Ok(k),
        Err(russh::keys::Error::KeyIsEncrypted) => match passphrase {
            None => {
                Err(CoreError::new("auth.secret_required", "Key passphrase is required").with("kind", "passphrase"))
            }
            Some(p) => russh::keys::decode_secret_key(text, Some(p))
                .map_err(|_| CoreError::new("auth.passphrase_wrong", "Wrong key passphrase")),
        },
        Err(e) => {
            Err(CoreError::new("auth.key_invalid", format!("Key is not recognized: {e}")).with("reason", e.to_string()))
        }
    }
}

pub fn read_key_file(path: &Path) -> CoreResult<zeroize::Zeroizing<String>> {
    std::fs::read_to_string(path)
        .map(zeroize::Zeroizing::new)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => {
                CoreError::new("auth.key_file_not_found", "Key file not found").with("path", path.display().to_string())
            }
            _ => CoreError::new("auth.key_file_unreadable", format!("Cannot read key file: {e}"))
                .with("path", path.display().to_string())
                .with("reason", e.to_string()),
        })
}

/// Сгенерировать ключ Ed25519 в формате OpenSSH (с фразой — шифрованный).
pub fn generate_ed25519_openssh(passphrase: Option<&str>) -> CoreResult<zeroize::Zeroizing<String>> {
    use rand::RngCore;
    use russh::keys::ssh_key::{private::Ed25519Keypair, Cipher, Kdf, LineEnding};
    let mut seed = zeroize::Zeroizing::new([0u8; 32]);
    rand::rng().fill_bytes(seed.as_mut());
    let mut key = PrivateKey::from(Ed25519Keypair::from_seed(&seed));
    if let Some(p) = passphrase {
        let mut salt = vec![0u8; 16];
        rand::rng().fill_bytes(&mut salt);
        key = key
            .encrypt_with(
                Cipher::Aes256Ctr,
                Kdf::Bcrypt { salt, rounds: 16 },
                rand::rng().next_u32(),
                p,
            )
            .map_err(CoreError::internal)?;
    }
    key.to_openssh(LineEnding::LF).map_err(CoreError::internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_variants() {
        let plain = generate_ed25519_openssh(None).unwrap();
        let d = decode_key(&plain, None).unwrap();
        assert_eq!(key_type(&d.algorithm()), "ssh-ed25519");
        assert!(fingerprint(d.public_key()).starts_with("SHA256:"));

        let enc = generate_ed25519_openssh(Some("pp")).unwrap();
        assert_eq!(decode_key(&enc, None).unwrap_err().code, "auth.secret_required");
        assert_eq!(decode_key(&enc, Some("bad")).unwrap_err().code, "auth.passphrase_wrong");
        assert!(decode_key(&enc, Some("pp")).is_ok());
        assert_eq!(decode_key("garbage", None).unwrap_err().code, "auth.key_invalid");
    }
}
