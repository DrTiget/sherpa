use crate::{model::*, secrets::SecretStore, CoreError, CoreResult};
use std::{path::PathBuf, sync::Arc};

/// Список серверов в JSON + секреты в SecretStore.
pub struct ServerStore {
    path: PathBuf,
    secrets: Arc<dyn SecretStore>,
}

fn sref(id: &str, kind: &str) -> String {
    format!("server:{id}:{kind}")
}

impl ServerStore {
    pub fn new(path: PathBuf, secrets: Arc<dyn SecretStore>) -> Self {
        Self { path, secrets }
    }

    pub fn secrets(&self) -> &Arc<dyn SecretStore> {
        &self.secrets
    }

    pub fn list(&self) -> CoreResult<Vec<Server>> {
        match std::fs::read_to_string(&self.path) {
            Ok(t) if t.trim().is_empty() => Ok(vec![]),
            Ok(t) => Ok(serde_json::from_str(&t)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
            Err(e) => Err(e.into()),
        }
    }

    fn save(&self, list: &[Server]) -> CoreResult<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(list)?)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> CoreResult<Server> {
        self.list()?.into_iter().find(|s| s.id == id).ok_or(CoreError::NotFound)
    }

    /// Кладёт секреты из формы в хранилище и возвращает конфиг со ссылками.
    /// `None` в поле секрета при обновлении — оставить прежнее значение.
    fn apply_auth(&self, id: &str, input: &AuthInput, old: Option<&AuthConfig>) -> CoreResult<AuthConfig> {
        let put = |kind: &str, val: &Option<String>, old_ref: Option<&String>| -> CoreResult<Option<String>> {
            match val {
                Some(v) if !v.is_empty() => {
                    let r = sref(id, kind);
                    self.secrets.set(&r, v)?;
                    Ok(Some(r))
                }
                _ => Ok(old_ref.cloned()),
            }
        };
        Ok(match input {
            AuthInput::Password { password } => {
                let o = if let Some(AuthConfig::Password { password_ref }) = old { password_ref.as_ref() } else { None };
                AuthConfig::Password { password_ref: put("password", password, o)? }
            }
            AuthInput::Key { key_path, key_pem, passphrase } => {
                let (ok, op) = if let Some(AuthConfig::Key { key_ref, passphrase_ref, .. }) = old {
                    (key_ref.as_ref(), passphrase_ref.as_ref())
                } else {
                    (None, None)
                };
                let path = key_path.clone().filter(|p| !p.is_empty());
                let pem = key_pem.clone().filter(|p| !p.is_empty());
                // выбран путь — импортированный ключ больше не нужен
                let key_ref = if pem.is_some() { put("key", &pem, ok)? } else if path.is_some() { None } else { ok.cloned() };
                if key_ref.is_none() {
                    self.secrets.delete(&sref(id, "key"))?;
                }
                AuthConfig::Key { key_path: path, key_ref, passphrase_ref: put("passphrase", passphrase, op)? }
            }
            AuthInput::Agent => AuthConfig::Agent,
        })
    }

    pub fn create(&self, input: ServerInput) -> CoreResult<Server> {
        let id = uuid::Uuid::new_v4().to_string();
        let auth = self.apply_auth(&id, &input.auth, None)?;
        let s = Server { id, name: input.name, host: input.host, port: input.port, username: input.username, group: input.group.filter(|g| !g.is_empty()), auth, enhanced_shell: input.enhanced_shell };
        let mut list = self.list()?;
        list.push(s.clone());
        self.save(&list)?;
        Ok(s)
    }

    pub fn update(&self, id: &str, input: ServerInput) -> CoreResult<Server> {
        let mut list = self.list()?;
        let pos = list.iter().position(|s| s.id == id).ok_or(CoreError::NotFound)?;
        let auth = self.apply_auth(id, &input.auth, Some(&list[pos].auth))?;
        let s = Server { id: id.into(), name: input.name, host: input.host, port: input.port, username: input.username, group: input.group.filter(|g| !g.is_empty()), auth, enhanced_shell: input.enhanced_shell };
        list[pos] = s.clone();
        self.save(&list)?;
        Ok(s)
    }

    pub fn set_enhanced_shell(&self, id: &str, on: bool) -> CoreResult<Server> {
        let mut list = self.list()?;
        let s = list.iter_mut().find(|s| s.id == id).ok_or(CoreError::NotFound)?;
        s.enhanced_shell = on;
        let s = s.clone();
        self.save(&list)?;
        Ok(s)
    }

    pub fn delete(&self, id: &str) -> CoreResult<()> {
        let mut list = self.list()?;
        let before = list.len();
        list.retain(|s| s.id != id);
        if list.len() == before {
            return Err(CoreError::NotFound);
        }
        for k in ["password", "passphrase", "key"] {
            self.secrets.delete(&sref(id, k))?;
        }
        self.save(&list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemoryStore;

    fn input(pw: Option<&str>) -> ServerInput {
        ServerInput { name: "a".into(), host: "h".into(), port: 22, username: "u".into(), group: Some("g".into()), enhanced_shell: false, auth: AuthInput::Password { password: pw.map(Into::into) } }
    }

    #[test]
    fn crud_keeps_secrets_out_of_json() {
        let dir = tempfile::tempdir().unwrap();
        let mem = Arc::new(MemoryStore::default());
        let st = ServerStore::new(dir.path().join("servers.json"), mem.clone());
        let s = st.create(input(Some("hunter2-secret"))).unwrap();
        let raw = std::fs::read_to_string(dir.path().join("servers.json")).unwrap();
        assert!(!raw.contains("hunter2-secret"));
        assert_eq!(mem.get(&format!("server:{}:password", s.id)).unwrap().as_deref(), Some("hunter2-secret"));
        // обновление без пароля сохраняет прежний
        let u = st.update(&s.id, input(None)).unwrap();
        assert_eq!(u.auth, s.auth);
        st.delete(&s.id).unwrap();
        assert!(st.list().unwrap().is_empty());
        assert_eq!(mem.get(&format!("server:{}:password", s.id)).unwrap(), None);
        assert!(matches!(st.delete(&s.id), Err(CoreError::NotFound)));
    }
}
