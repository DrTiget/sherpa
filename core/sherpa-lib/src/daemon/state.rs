//! Общее состояние демона.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::{mpsc, oneshot, watch};

use super::registry::{registry, MethodGroup};
use super::session::TermSession;
use super::Paths;
use crate::config::Settings;
use crate::error::{CoreError, CoreResult};
use crate::ssh::Pool;
use crate::store::Store;

/// Очередь исходящих строк клиента. Ограничена: медленный клиент тормозит
/// чтение из SSH-каналов своих сессий (backpressure), данные не теряются.
pub const CLIENT_QUEUE: usize = 32;
/// Сколько ждать ответа пользователя на вопрос (host key, пароль).
pub const PROMPT_TIMEOUT: Duration = Duration::from_secs(300);
/// Сколько живёт сессия без клиента (ждёт `session.attach`).
pub const DETACH_GRACE: Duration = Duration::from_secs(30);
/// Соединение пула без терминалов и каналов закрывается после простоя.
pub const POOL_IDLE: Duration = Duration::from_secs(600);

#[derive(Clone)]
pub struct ClientHandle {
    pub id: u64,
    pub tx: mpsc::Sender<String>,
}

impl ClientHandle {
    /// Отправить сообщение (ждёт места в очереди). false — клиент отключился.
    pub async fn send(&self, v: &Value) -> bool {
        self.tx.send(v.to_string()).await.is_ok()
    }
}

pub struct PromptSlot {
    pub tx: oneshot::Sender<Value>,
    pub session: Option<String>,
}

pub struct Subscription {
    pub client: u64,
    pub kind: &'static str,
    pub task: tokio::task::AbortHandle,
}

pub struct DaemonState {
    pub settings: Settings,
    pub paths: Paths,
    pub started: Instant,
    store: Mutex<Option<Store>>,
    pub sessions: Mutex<HashMap<String, Arc<TermSession>>>,
    pub prompts: Mutex<HashMap<String, PromptSlot>>,
    pub clients: Mutex<HashMap<u64, ClientHandle>>,
    pub subscriptions: Mutex<HashMap<String, Subscription>>,
    pub pool: Pool,
    pub registry: Vec<Arc<dyn MethodGroup>>,
    next_id: AtomicU64,
    last_activity: Mutex<Instant>,
    pub shutdown: watch::Sender<Option<String>>,
}

impl DaemonState {
    pub fn new(settings: Settings, paths: Paths) -> Arc<Self> {
        Arc::new(Self {
            settings,
            paths,
            started: Instant::now(),
            store: Mutex::new(None),
            sessions: Mutex::new(HashMap::new()),
            prompts: Mutex::new(HashMap::new()),
            clients: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(HashMap::new()),
            pool: Pool::default(),
            registry: registry(),
            next_id: AtomicU64::new(1),
            last_activity: Mutex::new(Instant::now()),
            shutdown: watch::channel(None).0,
        })
    }

    /// Уникальный в пределах демона идентификатор: `s1`, `p2`, `m3`…
    pub fn new_id(&self, prefix: &str) -> String {
        format!("{prefix}{}", self.next_id.fetch_add(1, Ordering::SeqCst))
    }

    pub fn new_client_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    pub fn touch(&self) {
        *self.last_activity.lock().expect("activity") = Instant::now();
    }

    pub fn idle_for(&self) -> Duration {
        self.last_activity.lock().expect("activity").elapsed()
    }

    /// Работа с хранилищем (открывается при первом обращении, коротко под мьютексом).
    pub fn with_store<R>(&self, f: impl FnOnce(&mut Store) -> CoreResult<R>) -> CoreResult<R> {
        let mut g = self.store.lock().expect("store");
        if g.is_none() {
            *g = Some(Store::open(&self.settings)?);
        }
        f(g.as_mut().expect("store"))
    }

    pub fn session(&self, id: &str) -> Option<Arc<TermSession>> {
        self.sessions.lock().expect("sessions").get(id).cloned()
    }

    pub fn session_ids(&self) -> Vec<String> {
        let mut v: Vec<String> = self.sessions.lock().expect("sessions").keys().cloned().collect();
        v.sort();
        v
    }

    /// Зарегистрировать вопрос пользователю; ответ придёт через `answer`.
    pub fn new_prompt(&self, session: Option<String>) -> (String, oneshot::Receiver<Value>) {
        let id = self.new_id("p");
        let (tx, rx) = oneshot::channel();
        self.prompts
            .lock()
            .expect("prompts")
            .insert(id.clone(), PromptSlot { tx, session });
        (id, rx)
    }

    pub fn drop_prompt(&self, id: &str) {
        self.prompts.lock().expect("prompts").remove(id);
    }

    pub fn answer(&self, prompt_id: &str, session: Option<&str>, value: Value) -> CoreResult<()> {
        let mut prompts = self.prompts.lock().expect("prompts");
        let matches = prompts
            .get(prompt_id)
            .is_some_and(|p| session.is_none() || p.session.as_deref() == session);
        let slot = if matches { prompts.remove(prompt_id) } else { None };
        match slot {
            Some(s) => {
                let _ = s.tx.send(value);
                Ok(())
            }
            None => {
                Err(CoreError::new("session.prompt_expired", "Prompt is no longer active").with("prompt_id", prompt_id))
            }
        }
    }

    pub fn request_shutdown(&self, reason: &str) {
        let _ = self.shutdown.send(Some(reason.to_string()));
    }

    /// Клиент отключился: его сессии ждут переподключения, подписки отменяются.
    pub fn client_gone(&self, client: u64) {
        self.clients.lock().expect("clients").remove(&client);
        let sessions: Vec<Arc<TermSession>> = self.sessions.lock().expect("sessions").values().cloned().collect();
        for s in sessions {
            s.detach_if(client);
        }
        self.subscriptions.lock().expect("subs").retain(|_, sub| {
            if sub.client == client {
                sub.task.abort();
                false
            } else {
                true
            }
        });
        self.touch();
    }
}
