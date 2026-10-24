//! Подключения (CORE_API §3.3, §6.1).

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::error::{CoreError, CoreResult};
use crate::repo::{group, tag};
use crate::secret::Secret;
use crate::store::Store;
use crate::util::{new_id, now, opt_str};

pub const MAX_NAME_LEN: usize = 128;
pub const MAX_DESCRIPTION_LEN: usize = 4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    Password,
    KeyFile,
    KeyText,
}

impl AuthMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            AuthMethod::Password => "password",
            AuthMethod::KeyFile => "key_file",
            AuthMethod::KeyText => "key_text",
        }
    }
    pub fn parse(s: &str) -> CoreResult<AuthMethod> {
        match s {
            "password" => Ok(AuthMethod::Password),
            "key_file" => Ok(AuthMethod::KeyFile),
            "key_text" => Ok(AuthMethod::KeyText),
            _ => Err(CoreError::invalid("auth.method", format!("unknown method {s}"))),
        }
    }
}

/// Последний результат проверки (§6.5).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Status {
    pub state: String,
    pub checked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<i64>,
    pub error: Option<Value>,
}

impl Status {
    pub fn unknown() -> Self {
        Status { state: "unknown".into(), checked_at: None, latency_ms: None, error: None }
    }
}

/// Подключение без расшифрованных секретов.
#[derive(Debug, Clone)]
pub struct Conn {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub auth_method: AuthMethod,
    pub key_path: Option<String>,
    pub key_type: Option<String>,
    pub public_fingerprint: Option<String>,
    pub secret_password: Option<Vec<u8>>,
    pub secret_key_text: Option<Vec<u8>>,
    pub secret_passphrase: Option<Vec<u8>>,
    pub description: Option<String>,
    pub group_id: Option<String>,
    pub source: String,
    pub status: Status,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Conn {
    pub fn auth_json(&self) -> Value {
        match self.auth_method {
            AuthMethod::Password => json!({ "method": "password", "has_password": self.secret_password.is_some() }),
            AuthMethod::KeyFile => json!({
                "method": "key_file",
                "key_path": self.key_path,
                "has_passphrase": self.secret_passphrase.is_some(),
            }),
            AuthMethod::KeyText => json!({
                "method": "key_text",
                "has_key_text": self.secret_key_text.is_some(),
                "key_type": self.key_type,
                "public_fingerprint": self.public_fingerprint,
                "has_passphrase": self.secret_passphrase.is_some(),
            }),
        }
    }

    /// Полное представление (§6.1).
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "host": self.host,
            "port": self.port,
            "user": self.user,
            "auth": self.auth_json(),
            "description": self.description,
            "group_id": self.group_id,
            "tags": self.tags,
            "status": self.status,
            "source": self.source,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
        })
    }

    /// Краткое представление для списка.
    pub fn to_list_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "host": self.host,
            "port": self.port,
            "user": self.user,
            "auth": self.auth_json(),
            "description": self.description,
            "group_id": self.group_id,
            "tags": self.tags,
            "status": { "state": self.status.state, "checked_at": self.status.checked_at },
        })
    }

    pub fn aad(id: &str, field: &str) -> String {
        format!("conn:{id}:{field}")
    }
}

const COLS: &str = "id, name, host, port, user, auth_method, key_path, key_type, public_fingerprint,
    secret_password, secret_key_text, secret_passphrase, description, group_id, source,
    status_state, status_checked_at, status_latency_ms, status_error, created_at, updated_at";

fn from_row(r: &Row) -> rusqlite::Result<Conn> {
    let method: String = r.get(5)?;
    let err: Option<String> = r.get(18)?;
    Ok(Conn {
        id: r.get(0)?,
        name: r.get(1)?,
        host: r.get(2)?,
        port: r.get::<_, i64>(3)? as u16,
        user: r.get(4)?,
        auth_method: AuthMethod::parse(&method).unwrap_or(AuthMethod::Password),
        key_path: r.get(6)?,
        key_type: r.get(7)?,
        public_fingerprint: r.get(8)?,
        secret_password: r.get(9)?,
        secret_key_text: r.get(10)?,
        secret_passphrase: r.get(11)?,
        description: r.get(12)?,
        group_id: r.get(13)?,
        source: r.get(14)?,
        status: Status {
            state: r.get(15)?,
            checked_at: r.get(16)?,
            latency_ms: r.get(17)?,
            error: err.and_then(|s| serde_json::from_str(&s).ok()),
        },
        tags: Vec::new(),
        created_at: r.get(19)?,
        updated_at: r.get(20)?,
    })
}

pub fn not_found(id: &str) -> CoreError {
    CoreError::new("conn.not_found", format!("Connection not found: {id}")).with("id", id)
}

fn load_tags(store: &Store, c: &mut Conn) -> CoreResult<()> {
    let mut st = store
        .conn
        .prepare_cached("SELECT tag FROM connection_tags WHERE connection_id = ?1 ORDER BY tag")?;
    c.tags = st.query_map([&c.id], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?;
    Ok(())
}

pub fn get(store: &Store, id: &str) -> CoreResult<Conn> {
    let mut c = store
        .conn
        .query_row(&format!("SELECT {COLS} FROM connections WHERE id = ?1"), [id], from_row)
        .optional()?
        .ok_or_else(|| not_found(id))?;
    load_tags(store, &mut c)?;
    Ok(c)
}

pub fn all(store: &Store) -> CoreResult<Vec<Conn>> {
    let mut st = store.conn.prepare(&format!("SELECT {COLS} FROM connections ORDER BY name_lc"))?;
    let mut rows = st.query_map([], from_row)?.collect::<Result<Vec<_>, _>>()?;
    drop(st);
    for c in rows.iter_mut() {
        load_tags(store, c)?;
    }
    Ok(rows)
}

pub fn find_by_name(store: &Store, name: &str) -> CoreResult<Option<String>> {
    Ok(store
        .conn
        .query_row("SELECT id FROM connections WHERE name_lc = ?1", [name.trim().to_lowercase()], |r| r.get(0))
        .optional()?)
}

// ---------- ввод ----------

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthInput {
    pub method: String,
    #[serde(default)]
    pub password: Option<Secret>,
    #[serde(default)]
    pub key_path: Option<String>,
    #[serde(default)]
    pub key_text: Option<Secret>,
    #[serde(default)]
    pub passphrase: Option<Secret>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnInput {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<i64>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub auth: Option<AuthInput>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

fn validate_name(name: &str) -> CoreResult<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err(CoreError::required("name"));
    }
    if n.chars().count() > MAX_NAME_LEN {
        return Err(CoreError::too_long("name", MAX_NAME_LEN));
    }
    Ok(n.to_string())
}

fn validate_host(host: &str) -> CoreResult<String> {
    let h = host.trim();
    if h.is_empty() {
        return Err(CoreError::required("host"));
    }
    if h.len() > 255 || h.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(CoreError::invalid("host", "not a host name or address"));
    }
    Ok(h.to_string())
}

fn validate_port(p: i64) -> CoreResult<u16> {
    if !(1..=65535).contains(&p) {
        return Err(CoreError::new("validation.out_of_range", "Port is out of range")
            .with("field", "port")
            .with("min", 1)
            .with("max", 65535));
    }
    Ok(p as u16)
}

fn validate_description(d: Option<String>) -> CoreResult<Option<String>> {
    match d {
        Some(d) if d.chars().count() > MAX_DESCRIPTION_LEN => Err(CoreError::too_long("description", MAX_DESCRIPTION_LEN)),
        Some(d) if d.trim().is_empty() => Ok(None),
        d => Ok(d),
    }
}

fn empty_to_none(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn check_name_free(store: &Store, name: &str, except: Option<&str>) -> CoreResult<()> {
    if let Some(existing) = find_by_name(store, name)? {
        if Some(existing.as_str()) != except {
            return Err(CoreError::new("conn.name_taken", format!("Connection name is taken: {name}"))
                .with("name", name)
                .with("existing_id", existing));
        }
    }
    Ok(())
}

/// Разобрать ключ из текста: (тип, отпечаток публичного ключа).
/// Зашифрованный ключ без фразы допустим — тип станет известен позже.
pub fn inspect_key_text(text: &str, passphrase: Option<&str>) -> CoreResult<(Option<String>, Option<String>)> {
    match crate::ssh::keys::decode_key(text, passphrase) {
        Ok(k) => Ok((Some(crate::ssh::keys::key_type(&k.algorithm())), Some(crate::ssh::keys::fingerprint(k.public_key())))),
        Err(e) if e.code == "auth.secret_required" => Ok((None, None)),
        Err(e) => Err(e),
    }
}

/// Изменение секретного поля.
#[derive(Debug)]
pub enum SecretChange {
    Keep,
    Clear,
    Set(Secret),
}

/// Нормализованные данные подключения, готовые к записи.
pub struct ConnDraft {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub auth_method: AuthMethod,
    pub key_path: Option<String>,
    pub key_type: Option<String>,
    pub public_fingerprint: Option<String>,
    pub password: SecretChange,
    pub key_text: SecretChange,
    pub passphrase: SecretChange,
    pub description: Option<String>,
    pub group_id: Option<String>,
    pub tags: Vec<String>,
    pub source: String,
}

impl ConnDraft {
    pub fn from_input(store: &Store, inp: ConnInput, source: &str) -> CoreResult<ConnDraft> {
        let name = validate_name(inp.name.as_deref().unwrap_or(""))?;
        let host = validate_host(inp.host.as_deref().unwrap_or(""))?;
        let port = validate_port(inp.port.unwrap_or(22))?;
        if let Some(g) = &inp.group_id {
            group::ensure_exists(store, g)?;
        }
        let mut d = ConnDraft {
            name,
            host,
            port,
            user: empty_to_none(inp.user),
            auth_method: AuthMethod::Password,
            key_path: None,
            key_type: None,
            public_fingerprint: None,
            password: SecretChange::Clear,
            key_text: SecretChange::Clear,
            passphrase: SecretChange::Clear,
            description: validate_description(inp.description)?,
            group_id: inp.group_id,
            tags: tag::normalize_all(&inp.tags.unwrap_or_default())?,
            source: source.to_string(),
        };
        if let Some(a) = inp.auth {
            d.apply_auth(a)?;
        }
        Ok(d)
    }

    fn apply_auth(&mut self, a: AuthInput) -> CoreResult<()> {
        let method = AuthMethod::parse(&a.method)?;
        self.auth_method = method;
        self.password = SecretChange::Clear;
        self.key_text = SecretChange::Clear;
        self.passphrase = a.passphrase.clone().map(SecretChange::Set).unwrap_or(SecretChange::Clear);
        self.key_path = None;
        self.key_type = None;
        self.public_fingerprint = None;
        match method {
            AuthMethod::Password => {
                self.passphrase = SecretChange::Clear;
                if let Some(p) = a.password {
                    self.password = SecretChange::Set(p);
                }
            }
            AuthMethod::KeyFile => {
                let p = empty_to_none(a.key_path).ok_or_else(|| CoreError::required("auth.key_path"))?;
                self.key_path = Some(p);
            }
            AuthMethod::KeyText => {
                let t = a.key_text.ok_or_else(|| CoreError::required("auth.key_text"))?;
                let (kt, fp) = inspect_key_text(t.expose(), a.passphrase.as_ref().map(|s| s.expose()))?;
                self.key_type = kt;
                self.public_fingerprint = fp;
                self.key_text = SecretChange::Set(t);
            }
        }
        Ok(())
    }
}

fn write_secret(store: &mut Store, id: &str, field: &str, ch: &SecretChange, cur: Option<Vec<u8>>) -> CoreResult<Option<Vec<u8>>> {
    match ch {
        SecretChange::Keep => Ok(cur),
        SecretChange::Clear => Ok(None),
        SecretChange::Set(s) => Ok(Some(store.encrypt(&Conn::aad(id, field), s.expose())?)),
    }
}

fn set_tags(store: &Store, id: &str, tags: &[String]) -> CoreResult<()> {
    store.conn.execute("DELETE FROM connection_tags WHERE connection_id = ?1", [id])?;
    for t in tags {
        store
            .conn
            .execute("INSERT INTO connection_tags(connection_id, tag) VALUES (?1, ?2)", [id, t])?;
    }
    Ok(())
}

/// Создать подключение. Вызывающий отвечает за транзакцию, если нужна.
pub fn insert(store: &mut Store, d: ConnDraft) -> CoreResult<Conn> {
    check_name_free(store, &d.name, None)?;
    let id = new_id();
    let pw = write_secret(store, &id, "password", &d.password, None)?;
    let kt = write_secret(store, &id, "key_text", &d.key_text, None)?;
    let pp = write_secret(store, &id, "passphrase", &d.passphrase, None)?;
    let ts = now();
    store.conn.execute(
        "INSERT INTO connections(id, name, name_lc, host, port, user, auth_method, key_path, key_type,
            public_fingerprint, secret_password, secret_key_text, secret_passphrase, description,
            group_id, source, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?17)",
        params![
            id, d.name, d.name.to_lowercase(), d.host, d.port, d.user, d.auth_method.as_str(), d.key_path,
            d.key_type, d.public_fingerprint, pw, kt, pp, d.description, d.group_id, d.source, ts
        ],
    )?;
    set_tags(store, &id, &d.tags)?;
    get(store, &id)
}

/// Частичное обновление (merge patch, §3.3).
pub fn update(store: &mut Store, id: &str, patch: &Map<String, Value>) -> CoreResult<Conn> {
    const ALLOWED: &[&str] = &["name", "host", "port", "user", "auth", "description", "group_id", "tags"];
    if let Some(k) = patch.keys().find(|k| !ALLOWED.contains(&k.as_str())) {
        return Err(CoreError::invalid(k, "unknown field"));
    }
    let cur = get(store, id)?;
    let mut d = ConnDraft {
        name: cur.name.clone(),
        host: cur.host.clone(),
        port: cur.port,
        user: cur.user.clone(),
        auth_method: cur.auth_method,
        key_path: cur.key_path.clone(),
        key_type: cur.key_type.clone(),
        public_fingerprint: cur.public_fingerprint.clone(),
        password: SecretChange::Keep,
        key_text: SecretChange::Keep,
        passphrase: SecretChange::Keep,
        description: cur.description.clone(),
        group_id: cur.group_id.clone(),
        tags: cur.tags.clone(),
        source: cur.source.clone(),
    };
    if let Some(v) = patch.get("name") {
        d.name = validate_name(&opt_str(v, "name")?.unwrap_or_default())?;
    }
    if let Some(v) = patch.get("host") {
        d.host = validate_host(&opt_str(v, "host")?.unwrap_or_default())?;
    }
    if let Some(v) = patch.get("port") {
        d.port = validate_port(v.as_i64().ok_or_else(|| CoreError::invalid("port", "expected integer"))?)?;
    }
    if let Some(v) = patch.get("user") {
        d.user = empty_to_none(opt_str(v, "user")?);
    }
    if let Some(v) = patch.get("description") {
        d.description = validate_description(opt_str(v, "description")?)?;
    }
    if let Some(v) = patch.get("group_id") {
        d.group_id = opt_str(v, "group_id")?;
        if let Some(g) = &d.group_id {
            group::ensure_exists(store, g)?;
        }
    }
    if let Some(v) = patch.get("tags") {
        let tags: Vec<String> = match v {
            Value::Null => vec![],
            v => serde_json::from_value(v.clone()).map_err(|_| CoreError::invalid("tags", "expected array of strings"))?,
        };
        d.tags = tag::normalize_all(&tags)?;
    }
    if let Some(v) = patch.get("auth") {
        apply_auth_patch(store, &cur, &mut d, v)?;
    }
    check_name_free(store, &d.name, Some(id))?;

    let pw = write_secret(store, id, "password", &d.password, cur.secret_password.clone())?;
    let kt = write_secret(store, id, "key_text", &d.key_text, cur.secret_key_text.clone())?;
    let pp = write_secret(store, id, "passphrase", &d.passphrase, cur.secret_passphrase.clone())?;
    let addr_changed = d.host != cur.host || d.port != cur.port;
    let tx = store.conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE connections SET name=?2, name_lc=?3, host=?4, port=?5, user=?6, auth_method=?7, key_path=?8,
            key_type=?9, public_fingerprint=?10, secret_password=?11, secret_key_text=?12,
            secret_passphrase=?13, description=?14, group_id=?15, updated_at=?16
         WHERE id=?1",
        params![
            id, d.name, d.name.to_lowercase(), d.host, d.port, d.user, d.auth_method.as_str(), d.key_path,
            d.key_type, d.public_fingerprint, pw, kt, pp, d.description, d.group_id, now()
        ],
    )?;
    if addr_changed {
        tx.execute(
            "UPDATE connections SET status_state='unknown', status_checked_at=NULL, status_latency_ms=NULL,
                status_error=NULL WHERE id=?1",
            [id],
        )?;
    }
    tx.commit()?;
    set_tags(store, id, &d.tags)?;
    get(store, id)
}

fn apply_auth_patch(store: &mut Store, cur: &Conn, d: &mut ConnDraft, v: &Value) -> CoreResult<()> {
    let m = v.as_object().ok_or_else(|| CoreError::invalid("auth", "expected object"))?;
    const AUTH_FIELDS: &[&str] = &["method", "password", "key_path", "key_text", "passphrase"];
    if let Some(k) = m.keys().find(|k| !AUTH_FIELDS.contains(&k.as_str())) {
        return Err(CoreError::invalid(&format!("auth.{k}"), "unknown field"));
    }
    let secret = |key: &str| -> CoreResult<SecretChange> {
        Ok(match m.get(key) {
            None => SecretChange::Keep,
            Some(Value::Null) => SecretChange::Clear,
            Some(Value::String(s)) => SecretChange::Set(Secret::new(s.clone())),
            Some(_) => return Err(CoreError::invalid(&format!("auth.{key}"), "expected string or null")),
        })
    };
    let method = match m.get("method") {
        Some(v) => AuthMethod::parse(v.as_str().ok_or_else(|| CoreError::invalid("auth.method", "expected string"))?)?,
        None => cur.auth_method,
    };
    if method != cur.auth_method {
        // Новый метод: секреты прошлого удаляются, данные нового обязательны.
        let inp: AuthInput = crate::util::parse(v.clone())?;
        d.apply_auth(inp)?;
        return Ok(());
    }
    match method {
        AuthMethod::Password => {
            d.password = secret("password")?;
        }
        AuthMethod::KeyFile => {
            if let Some(v) = m.get("key_path") {
                d.key_path = Some(empty_to_none(opt_str(v, "auth.key_path")?).ok_or_else(|| CoreError::required("auth.key_path"))?);
            }
            d.passphrase = secret("passphrase")?;
        }
        AuthMethod::KeyText => {
            d.key_text = secret("key_text")?;
            d.passphrase = secret("passphrase")?;
            if matches!(d.key_text, SecretChange::Clear) {
                return Err(CoreError::required("auth.key_text"));
            }
            // Перепроверить ключ, если сменился текст или фраза.
            if !matches!((&d.key_text, &d.passphrase), (SecretChange::Keep, SecretChange::Keep)) {
                let text = match &d.key_text {
                    SecretChange::Set(s) => s.clone(),
                    _ => match &cur.secret_key_text {
                        Some(b) => store.decrypt_string(&Conn::aad(&cur.id, "key_text"), b)?,
                        None => return Err(CoreError::required("auth.key_text")),
                    },
                };
                let pass = match &d.passphrase {
                    SecretChange::Set(s) => Some(s.clone()),
                    SecretChange::Clear => None,
                    SecretChange::Keep => match &cur.secret_passphrase {
                        Some(b) => Some(store.decrypt_string(&Conn::aad(&cur.id, "passphrase"), b)?),
                        None => None,
                    },
                };
                let (kt, fp) = inspect_key_text(text.expose(), pass.as_ref().map(|s| s.expose()))?;
                d.key_type = kt;
                d.public_fingerprint = fp;
            }
        }
    }
    Ok(())
}

pub fn delete(store: &Store, ids: &[String]) -> CoreResult<Value> {
    let mut deleted = Vec::new();
    let mut missing = Vec::new();
    for id in ids {
        if store.conn.execute("DELETE FROM connections WHERE id = ?1", [id])? > 0 {
            deleted.push(id.clone());
        } else {
            missing.push(id.clone());
        }
    }
    Ok(json!({ "deleted": deleted, "not_found": missing }))
}

/// Копия со всеми полями и секретами; имя `"<name> (copy)"`, `"(copy 2)"`…
pub fn duplicate(store: &mut Store, id: &str, name: Option<&str>) -> CoreResult<Conn> {
    let src = get(store, id)?;
    let name = match name {
        Some(n) => validate_name(n)?,
        None => {
            let mut i = 1;
            loop {
                let cand = if i == 1 { format!("{} (copy)", src.name) } else { format!("{} (copy {i})", src.name) };
                if find_by_name(store, &cand)?.is_none() {
                    break cand;
                }
                i += 1;
            }
        }
    };
    let dec = |store: &mut Store, field: &str, b: &Option<Vec<u8>>| -> CoreResult<SecretChange> {
        Ok(match b {
            Some(b) => SecretChange::Set(store.decrypt_string(&Conn::aad(&src.id, field), b)?),
            None => SecretChange::Clear,
        })
    };
    let d = ConnDraft {
        name,
        host: src.host.clone(),
        port: src.port,
        user: src.user.clone(),
        auth_method: src.auth_method,
        key_path: src.key_path.clone(),
        key_type: src.key_type.clone(),
        public_fingerprint: src.public_fingerprint.clone(),
        password: dec(store, "password", &src.secret_password)?,
        key_text: dec(store, "key_text", &src.secret_key_text)?,
        passphrase: dec(store, "passphrase", &src.secret_passphrase)?,
        description: src.description.clone(),
        group_id: src.group_id.clone(),
        tags: src.tags.clone(),
        source: "duplicate".into(),
    };
    insert(store, d)
}

pub fn save_status(store: &Store, id: &str, st: &Status) -> CoreResult<()> {
    store.conn.execute(
        "UPDATE connections SET status_state=?2, status_checked_at=?3, status_latency_ms=?4, status_error=?5 WHERE id=?1",
        params![
            id,
            st.state,
            st.checked_at,
            st.latency_ms,
            st.error.as_ref().map(|e| e.to_string())
        ],
    )?;
    Ok(())
}

// ---------- список с фильтрами ----------

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListFilter {
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub no_group: bool,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default)]
    pub desc: bool,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
}

pub fn list(store: &Store, f: &ListFilter) -> CoreResult<(Vec<Conn>, usize)> {
    let groups = match &f.group_id {
        Some(g) => {
            group::ensure_exists(store, g)?;
            Some(if f.recursive { group::subtree(store, g)? } else { [g.clone()].into() })
        }
        None => None,
    };
    let tags = tag::normalize_all(&f.tags)?;
    let q = f.q.as_ref().map(|s| s.to_lowercase());
    let host = f.host.as_ref().map(|s| s.to_lowercase());
    if let Some(s) = &f.status {
        if !["online", "offline", "auth_failed", "error", "unknown"].contains(&s.as_str()) {
            return Err(CoreError::invalid_args("--status", format!("unknown status {s}")));
        }
    }
    let mut items: Vec<Conn> = all(store)?
        .into_iter()
        .filter(|c| {
            q.as_ref().is_none_or(|q| c.name.to_lowercase().contains(q) || c.host.to_lowercase().contains(q))
                && host.as_ref().is_none_or(|h| c.host.to_lowercase().contains(h))
                && tags.iter().all(|t| c.tags.contains(t))
                && (!f.no_group || c.group_id.is_none())
                && groups.as_ref().is_none_or(|g| c.group_id.as_ref().is_some_and(|id| g.contains(id)))
                && f.status.as_ref().is_none_or(|s| &c.status.state == s)
        })
        .collect();
    match f.sort.as_deref().unwrap_or("name") {
        "name" => items.sort_by_key(|c| c.name.to_lowercase()),
        "host" => items.sort_by(|a, b| a.host.cmp(&b.host).then(a.port.cmp(&b.port))),
        "status" => items.sort_by(|a, b| a.status.state.cmp(&b.status.state).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))),
        "last_checked" => items.sort_by(|a, b| a.status.checked_at.cmp(&b.status.checked_at)),
        other => return Err(CoreError::invalid_args("--sort", format!("unknown sort {other}"))),
    }
    if f.desc {
        items.reverse();
    }
    let total = items.len();
    let items = items
        .into_iter()
        .skip(f.offset.unwrap_or(0))
        .take(f.limit.unwrap_or(usize::MAX))
        .collect();
    Ok((items, total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::testutil::temp_store;

    fn mk(store: &mut Store, v: Value) -> CoreResult<Conn> {
        let inp: ConnInput = serde_json::from_value(v).unwrap();
        let d = ConnDraft::from_input(store, inp, "manual")?;
        insert(store, d)
    }

    #[test]
    fn crud_and_secrets_hidden() {
        let (_d, mut s) = temp_store();
        let c = mk(&mut s, json!({"name":"web-1","host":"10.0.0.5","tags":["Prod","nginx"],
            "auth":{"method":"password","password":"s3cr3t"}})).unwrap();
        let j = c.to_json();
        assert_eq!(j["auth"]["has_password"], true);
        assert!(!j.to_string().contains("s3cr3t"));
        assert_eq!(j["tags"], json!(["nginx", "prod"]));
        assert_eq!(mk(&mut s, json!({"name":"WEB-1","host":"x"})).unwrap_err().code, "conn.name_taken");
        assert_eq!(mk(&mut s, json!({"name":"p","host":"x","port":70000})).unwrap_err().code, "validation.out_of_range");

        let pw = s.decrypt_string(&Conn::aad(&c.id, "password"), c.secret_password.as_ref().unwrap()).unwrap();
        assert_eq!(pw.expose(), "s3cr3t");

        let patch = json!({"port": 2222, "auth": {"password": null}, "description": "d"});
        let u = update(&mut s, &c.id, patch.as_object().unwrap()).unwrap();
        assert_eq!(u.port, 2222);
        assert!(u.secret_password.is_none());
        assert_eq!(u.status.state, "unknown");

        let dup = duplicate(&mut s, &c.id, None).unwrap();
        assert_eq!(dup.name, "web-1 (copy)");
        assert_eq!(duplicate(&mut s, &c.id, None).unwrap().name, "web-1 (copy 2)");

        let r = delete(&s, &[c.id.clone(), "nope".into()]).unwrap();
        assert_eq!(r["not_found"], json!(["nope"]));
    }

    #[test]
    fn list_filters() {
        let (_d, mut s) = temp_store();
        let g = group::create(&s, "G", None).unwrap();
        let sub = group::create(&s, "S", Some(&g.id)).unwrap();
        mk(&mut s, json!({"name":"web-1","host":"10.0.0.1","tags":["prod","web"],"group_id":g.id})).unwrap();
        mk(&mut s, json!({"name":"db-1","host":"db.local","tags":["prod"],"group_id":sub.id})).unwrap();
        mk(&mut s, json!({"name":"misc","host":"10.9.9.9"})).unwrap();
        let f = |v: Value| list(&s, &serde_json::from_value(v).unwrap()).unwrap().1;
        assert_eq!(f(json!({})), 3);
        assert_eq!(f(json!({"q":"DB"})), 1);
        assert_eq!(f(json!({"q":"10.0"})), 1);
        assert_eq!(f(json!({"host":"10."})), 2);
        assert_eq!(f(json!({"tags":["prod","web"]})), 1);
        assert_eq!(f(json!({"group_id":g.id})), 1);
        assert_eq!(f(json!({"group_id":g.id,"recursive":true})), 2);
        assert_eq!(f(json!({"no_group":true})), 1);
        assert_eq!(f(json!({"status":"unknown"})), 3);
    }

    #[test]
    fn switching_auth_method_drops_old_secrets() {
        let (_d, mut s) = temp_store();
        let c = mk(&mut s, json!({"name":"a","host":"h","auth":{"method":"password","password":"x"}})).unwrap();
        let patch = json!({"auth":{"method":"key_file","key_path":"/k"}});
        let u = update(&mut s, &c.id, patch.as_object().unwrap()).unwrap();
        assert!(u.secret_password.is_none());
        assert_eq!(u.key_path.as_deref(), Some("/k"));
        let bad = json!({"auth":{"method":"key_text"}});
        assert_eq!(update(&mut s, &c.id, bad.as_object().unwrap()).unwrap_err().code, "validation.required");
    }
}
