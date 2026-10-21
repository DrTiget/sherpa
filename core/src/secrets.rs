use crate::CoreResult;
use std::{collections::HashMap, sync::Mutex};

/// Абстракция над системным хранилищем секретов (в тестах — память).
pub trait SecretStore: Send + Sync {
    fn set(&self, name: &str, value: &str) -> CoreResult<()>;
    fn get(&self, name: &str) -> CoreResult<Option<String>>;
    fn delete(&self, name: &str) -> CoreResult<()>;
}

/// Windows Credential Manager / macOS Keychain / Secret Service (Linux).
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self { service: service.into() }
    }
    fn entry(&self, name: &str) -> CoreResult<keyring::Entry> {
        Ok(keyring::Entry::new(&self.service, name)?)
    }
}

impl SecretStore for KeyringStore {
    fn set(&self, name: &str, value: &str) -> CoreResult<()> {
        Ok(self.entry(name)?.set_password(value)?)
    }
    fn get(&self, name: &str) -> CoreResult<Option<String>> {
        match self.entry(name)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    fn delete(&self, name: &str) -> CoreResult<()> {
        match self.entry(name)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn set(&self, name: &str, value: &str) -> CoreResult<()> {
        self.0.lock().unwrap().insert(name.into(), value.into());
        Ok(())
    }
    fn get(&self, name: &str) -> CoreResult<Option<String>> {
        Ok(self.0.lock().unwrap().get(name).cloned())
    }
    fn delete(&self, name: &str) -> CoreResult<()> {
        self.0.lock().unwrap().remove(name);
        Ok(())
    }
}
