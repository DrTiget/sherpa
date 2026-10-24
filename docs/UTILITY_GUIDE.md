# Как добавить утилиту рабочего стола в ядро

Эталон — `core/sherpa-lib/src/utilities/monitor/`. Копируйте его структуру.
Контракт демона — `docs/CORE_API.md` §4, список утилит — `docs/design/DESKTOP.md`.

## 1. Из чего состоит утилита

```
core/sherpa-lib/src/utilities/<name>/
├── mod.rs     команда для сервера, сборщик, MethodGroup (методы демона), тесты на FakeExec
└── parse.rs   чистые функции разбора вывода + фикстуры и тесты
```

Утилита **не открывает SSH сама**. Она получает соединение из пула
(`cx.conn(id)`) и выполняет команды через трейт `ssh::RemoteExec`:

```rust
let out = conn.exec("df -PB1").await?;               // обычная команда
let out = conn.exec_privileged("journalctl …").await?; // с правами root (sudo по правилам priv.*)
conn.ensure_writable()?;                              // перед ЛЮБЫМ изменением на сервере
```

`ExecOutput { stdout, stderr, exit_status }`; `out.success()`,
`out.stdout_lossy()`.

## 2. Пошагово

1. **Команда.** Одна команда (или скрипт `sh`) на вызов, без интерактива.
   Начинайте с `LC_ALL=C; export LC_ALL;`. Несколько источников — склеивайте
   через разделитель (`echo @@SHERPA@@`), как в `monitor::script()`.
   Пользовательские значения подставляйте только через `ssh::shell_quote()`.
   Тяжёлое (сканы диска) — `nice -n 19 ionice -c3 …`.
2. **Разбор (`parse.rs`).** Чистые функции `&str → структура`
   (`#[derive(Serialize)]`). Никакого текста наружу: фронт получает только
   JSON. Неожиданный вывод — ошибка `remote.bad_output`, не паника.
   Числа — в базовых единицах (`*_bytes`, `*_sec`, `*_pct`).
3. **Сборщик.** `async fn …<E: RemoteExec>(exec: &E, …) -> CoreResult<Value>` —
   обобщённый по `RemoteExec`, чтобы тестировать на `FakeExec`.
   Код выхода ≠ 0 → `utilities::command_failed(name, &out)`.
   Нет нужной программы/файла → `cap.missing` с `capability`.
4. **Методы демона.** `struct XxxMethods; impl MethodGroup` с
   `prefix() = "<name>"`, `methods() = &["<name>.list", …]`, `call()`.
   Параметры — помощники `daemon::proto::{req_str, opt_str, opt_u64, opt_bool}`.
   Соединение — `cx.conn(&req_str(&p, "connection_id")?).await?`.
5. **Регистрация.** `pub mod <name>;` в `utilities/mod.rs` и строка
   `Arc::new(crate::utilities::<name>::XxxMethods)` в
   `daemon::registry::registry()`.
6. **Capabilities.** Если утилите нужна программа, добавьте её в
   `utilities::capabilities::TOOLS` и флаг в `utilities` внутри `parse()`.
7. **Контракт.** Раздел в `docs/CORE_API.md` §4 (методы, параметры, пример
   ответа) и новые коды ошибок в §5.

## 3. Правила

- **Опасные операции** (удалить, остановить, применить сеть): сначала
  `conn.ensure_writable()?`; требуют root — `exec_privileged`; фронт сам
  спрашивает подтверждение. Не делайте молча то, что не просили.
- **Потоки** (`*.subscribe`, `*.follow`): как `monitor.subscribe` — задача
  `tokio::spawn`, события `event("<name>", {subscription, …})` через
  `cx.client.send()`, регистрация в `cx.state.subscriptions` (отмена при
  отключении клиента). Интервал не меньше 1 с. Ошибка — событие `error` с
  `fatal: true`, задача завершается.
- **Секреты** не попадают в логи, ответы и ошибки (`stderr` обрезан до
  500 символов — проверьте, что там не может быть пароля).
- **Ошибки** — только `CoreError` с кодом `область.причина` и `params`;
  `message` по-английски. Новый код — в каталог CORE_API §5.
- **Без паник**: никаких `unwrap()` на данных с сервера.
- Порядок и стиль — как в `monitor`; `cargo fmt`, `cargo clippy -- -D warnings`.

## 4. Тесты (обязательно)

1. `parse.rs`: фикстуры — реальный вывод команд (Debian/Ubuntu, Alpine/BusyBox
   если отличается), граничные случаи, мусор на входе.
2. `mod.rs`: сборщик на `FakeExec`:

```rust
let fake = FakeExec::new().on("df -PB1", FIXTURE, 0);   // ответ по подстроке команды
let data = collect(&fake, &opts).await.unwrap();
assert_eq!(data["items"][0]["mount"], "/");
assert!(fake.calls()[0].contains("LC_ALL=C"));           // какая команда ушла
let fake = FakeExec::new().on("df", "", 1);
assert_eq!(collect(&fake, &opts).await.unwrap_err().code, "remote.command_failed");
```

3. При желании — сквозной тест через демон: `core/sherpa-core/tests/daemon.rs`
   (`desktop_monitor_priv`) — тестовый SSH-сервер с подменёнными ответами
   `Options { exec: Some(responder) }`.

Проверка перед коммитом (из `core/`):

```sh
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings && cargo test
```

## 5. Чек-лист

- [ ] `utilities/<name>/{mod.rs,parse.rs}`, методы `<name>.*`
- [ ] зарегистрировано в `daemon::registry::registry()`
- [ ] изменения — через `ensure_writable()`, root — через `exec_privileged()`
- [ ] тесты разбора и сборщика на `FakeExec`
- [ ] CORE_API: §4 (методы) и §5 (коды)
- [ ] fmt / clippy / test зелёные


## Общие подтверждения опасных действий

Механизм — `daemon::methods::danger::Confirmations`, экземпляр
`cx.state.confirmations`. Не создавайте собственные токены или таблицы.

1. Фронт вызывает `danger.prepare {method: "files.delete", params:
   {connection_id, path, recursive: true}}`.
2. Показывает пользователю действие и параметры. При согласии вызывает
   исходный метод с теми же параметрами плюс выданный `confirm_token`.
3. Обработчик проверяет параметры и `conn.ensure_writable()?`, затем
   **до первого изменения** вызывает:

```rust
cx.state.confirmations.consume(cx.client.id, method, &params)?;
```

Проверка и потребление атомарны. Повтор, чужой клиент, другое действие,
изменённые параметры или истёкшие 60 секунд —
`danger.confirmation_required`. Подтверждение не заменяет проверку прав.
Токен остаётся израсходованным и при ошибке удалённой операции: для повтора
нужно новое подтверждение. В памяти до 1024 токенов; после отключения
клиента они удаляются. `confirm_token` исключается из сравниваемого объекта,
прочие параметры должны совпасть (отсутствующее поле и явный default различны).

Файлы получают канал через `conn.sftp().await?`: RAII-объект удерживает
разрешение общего пула и закрывает подсистему при отмене/выходе. Новые
TCP-каналы удерживают `conn.channel_permit()` на всё время передачи.
Передачи и туннели находятся в `state.jobs`: переживают отключение GUI,
отменяются при остановке демона. Их события — уведомления о состоянии;
если очередь GUI заполнена, актуальное состояние восстанавливается list.
Это отличается от терминального вывода, который нельзя терять.
