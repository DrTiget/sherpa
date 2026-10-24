//! Ключ-файл и шифрование секретных полей (XChaCha20-Poly1305), ТЗ §4.5.
//!
//! Формат ключ-файла (текст): `sherpa-key-v1\n<base64 32 байта>\n`.
//! Формат шифротекста: `0x01 || nonce(24) || ciphertext+tag`.
//! AAD связывает шифротекст с записью и полем (`conn:<id>:password`),
//! поэтому переставить секреты между записями нельзя.

use std::io::Write;
use std::path::Path;

use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::error::{CoreError, CoreResult};
use crate::protocol::Warning;

const HEADER: &str = "sherpa-key-v1";
const BLOB_VERSION: u8 = 1;
const NONCE_LEN: usize = 24;

/// Ключ шифрования в памяти; затирается при удалении.
pub struct MasterKey {
    bytes: Zeroizing<[u8; 32]>,
}

impl MasterKey {
    pub fn generate() -> Self {
        let mut bytes = Zeroizing::new([0u8; 32]);
        rand::rng().fill_bytes(bytes.as_mut());
        Self { bytes }
    }

    /// Отпечаток ключа `SHA256:<base64>` — для сверки с БД, ключ не раскрывает.
    pub fn fingerprint(&self) -> String {
        let mut h = Sha256::new();
        h.update(b"sherpa-key-fingerprint:");
        h.update(self.bytes.as_ref());
        format!(
            "SHA256:{}",
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(h.finalize())
        )
    }

    fn encode(&self) -> Zeroizing<String> {
        Zeroizing::new(format!(
            "{HEADER}\n{}\n",
            base64::engine::general_purpose::STANDARD.encode(self.bytes.as_ref())
        ))
    }

    pub fn load(path: &Path) -> CoreResult<MasterKey> {
        let text = Zeroizing::new(std::fs::read_to_string(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => CoreError::new("store.key_file_missing", "Key file not found")
                .with("path", path.display().to_string()),
            std::io::ErrorKind::PermissionDenied => CoreError::fs("store.permission_denied", path, &e),
            _ => CoreError::fs("fs.read_failed", path, &e),
        })?);
        let invalid = || {
            CoreError::new("crypto.key_file_invalid", "Key file is invalid")
                .with("path", path.display().to_string())
        };
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(HEADER) {
            return Err(invalid());
        }
        let raw = Zeroizing::new(
            base64::engine::general_purpose::STANDARD
                .decode(lines.next().unwrap_or("").trim())
                .map_err(|_| invalid())?,
        );
        if raw.len() != 32 {
            return Err(invalid());
        }
        let mut bytes = Zeroizing::new([0u8; 32]);
        bytes.copy_from_slice(&raw);
        Ok(MasterKey { bytes })
    }

    /// Записать ключ в новый файл с правами 600. Существующий файл не трогает.
    pub fn write_new(&self, path: &Path) -> CoreResult<()> {
        if let Some(dir) = path.parent() {
            create_private_dir(dir)?;
        }
        let mut f = open_private_new(path)?;
        f.write_all(self.encode().as_bytes())
            .and_then(|_| f.sync_all())
            .map_err(|e| CoreError::fs("fs.write_failed", path, &e))
    }

    pub fn encrypt(&self, aad: &str, plain: &[u8]) -> CoreResult<Vec<u8>> {
        let cipher = XChaCha20Poly1305::new(self.bytes.as_ref().into());
        let mut nonce = [0u8; NONCE_LEN];
        rand::rng().fill_bytes(&mut nonce);
        let ct = cipher
            .encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad: aad.as_bytes() })
            .map_err(|_| CoreError::internal("encryption failed"))?;
        let mut out = Vec::with_capacity(1 + NONCE_LEN + ct.len());
        out.push(BLOB_VERSION);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        Ok(out)
    }

    pub fn decrypt(&self, aad: &str, blob: &[u8]) -> CoreResult<Zeroizing<Vec<u8>>> {
        let field = aad.rsplit(':').next().unwrap_or(aad).to_string();
        let fail = || CoreError::new("crypto.decrypt_failed", "Failed to decrypt secret").with("field", field.clone());
        if blob.len() < 1 + NONCE_LEN + 16 || blob[0] != BLOB_VERSION {
            return Err(fail());
        }
        let cipher = XChaCha20Poly1305::new(self.bytes.as_ref().into());
        cipher
            .decrypt(
                XNonce::from_slice(&blob[1..1 + NONCE_LEN]),
                Payload { msg: &blob[1 + NONCE_LEN..], aad: aad.as_bytes() },
            )
            .map(Zeroizing::new)
            .map_err(|_| fail())
    }
}

/// Предупреждение, если права ключ-файла шире 600 (на Windows не проверяется — ACL).
pub fn permission_warning(path: &Path) -> Option<Warning> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path).ok()?.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Some(
                CoreError::new("crypto.key_file_insecure", format!("Key file permissions are too open: {mode:o}"))
                    .with("path", path.display().to_string())
                    .with("mode", format!("{mode:o}")),
            );
        }
        None
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

pub fn permissions_ok(path: &Path) -> bool {
    permission_warning(path).is_none()
}

pub fn create_private_dir(dir: &Path) -> CoreResult<()> {
    if dir.as_os_str().is_empty() || dir.exists() {
        return Ok(());
    }
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(dir).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => CoreError::fs("store.permission_denied", dir, &e),
        _ => CoreError::fs("fs.write_failed", dir, &e),
    })
}

/// Открыть новый файл (ошибка `fs.exists`, если есть) с правами 600.
pub fn open_private_new(path: &Path) -> CoreResult<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => CoreError::new("fs.exists", "File already exists")
            .with("path", path.display().to_string()),
        std::io::ErrorKind::PermissionDenied => CoreError::fs("store.permission_denied", path, &e),
        _ => CoreError::fs("fs.write_failed", path, &e),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_aad_binding() {
        let k = MasterKey::generate();
        let blob = k.encrypt("conn:1:password", b"hunter2").unwrap();
        assert_eq!(&*k.decrypt("conn:1:password", &blob).unwrap(), b"hunter2");
        assert_eq!(k.decrypt("conn:2:password", &blob).unwrap_err().code, "crypto.decrypt_failed");
        let other = MasterKey::generate();
        assert!(other.decrypt("conn:1:password", &blob).is_err());
    }

    #[test]
    fn key_file_write_load_and_perms() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sub/k.key");
        let k = MasterKey::generate();
        k.write_new(&p).unwrap();
        assert_eq!(MasterKey::load(&p).unwrap().fingerprint(), k.fingerprint());
        assert!(permissions_ok(&p));
        assert_eq!(k.write_new(&p).unwrap_err().code, "fs.exists");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert_eq!(permission_warning(&p).unwrap().code, "crypto.key_file_insecure");
        }
        std::fs::write(&p, "garbage").unwrap();
        assert_eq!(MasterKey::load(&p).err().unwrap().code, "crypto.key_file_invalid");
    }
}
