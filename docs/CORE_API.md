# Sherpa — контракт ядра `sherpa-core` (MVP, протокол v1)

Документ описывает интерфейс ядра для фронтов (GTK, WinUI, позже Laravel).
Основа — `docs/REQUIREMENTS.md` (§3.3, §4). Всё, что здесь не описано, фронт
делать не должен: бизнес-логика только в ядре.

---

## 1. Общие соглашения

### 1.1 Формат вызова

```
sherpa-core [глобальные флаги] <группа> <действие> [флаги команды]
```

Пример: `sherpa-core conn list --tag prod`.

- Параметры передаются **флагами** (простые значения) или **JSON в stdin**
  (флаг `--input -`). Если указаны оба способа, значения из stdin перекрывают
  флаги. Секреты — только через stdin (см. §2).
- Ядро пишет в stdout **ровно один JSON-объект** и перевод строки
  (кроме `conn check --stream`, см. §3.6, и `daemon`, см. §4). stderr — только диагностический лог, без секретов;
  фронт его не разбирает.
- Кодировка — UTF-8. Время — RFC 3339 в UTC (`2026-10-24T09:15:00Z`).
  Идентификаторы — UUID v7 строкой.

### 1.2 Глобальные флаги

| Флаг | Переменная окружения | По умолчанию | Назначение |
|------|---------------------|--------------|------------|
| `--config <path>` | `SHERPA_CONFIG` | см. ниже | Конфиг ядра (TOML) |
| `--db <path>` | `SHERPA_DB` | из конфига | Путь к SQLite |
| `--key-file <path>` | `SHERPA_KEY_FILE` | из конфига | Путь к ключ-файлу |
| `--input -` | — | — | Читать параметры команды JSON-объектом из stdin |
| `--pretty` | — | выкл. | Форматированный JSON (для отладки) |
| `--log-level <lvl>` | `SHERPA_LOG` | `warn` | `error/warn/info/debug` в stderr |

Пути по умолчанию:

| ОС | Конфиг | БД | Ключ-файл |
|----|--------|----|-----------|
| Linux | `$XDG_CONFIG_HOME/sherpa/core.toml` | `$XDG_DATA_HOME/sherpa/sherpa.db` | `$XDG_CONFIG_HOME/sherpa/sherpa.key` |
| Windows | `%APPDATA%\Sherpa\core.toml` | `%LOCALAPPDATA%\Sherpa\sherpa.db` | `%APPDATA%\Sherpa\sherpa.key` |

Конфиг (все поля необязательны):

```toml
db = "/path/sherpa.db"
key_file = "/path/sherpa.key"
connect_timeout_sec = 10      # таймаут проверки/подключения
check_concurrency = 32        # параллельных проверок в пачке
daemon_idle_timeout_sec = 300 # простой демона без сессий
```

Флага `--lang` нет: ядро возвращает коды, тексты локализует фронт.

### 1.3 Успешный ответ

```json
{ "ok": true, "protocol": 1, "data": { ... } }
```

`data` — объект или `null`. Списки всегда в поле объекта (`{"items": [...]}`),
чтобы можно было добавлять поля без поломки.

Необязательное поле `warnings` (и в успешном ответе, и в ошибке) — список
предупреждений в формате ошибки §1.4: выполнению они не мешают, фронт может
их показать (например, `crypto.key_file_insecure`).

```json
{ "ok": true, "protocol": 1, "data": { ... },
  "warnings": [ { "code": "crypto.key_file_insecure", "message": "...",
                  "params": { "path": "...", "mode": "644" } } ] }
```

### 1.4 Ошибка

```json
{
  "ok": false,
  "protocol": 1,
  "error": {
    "code": "conn.not_found",
    "message": "Connection not found: 0192f0c4-...",
    "params": { "id": "0192f0c4-..." },
    "details": [ ]
  }
}
```

- `code` — стабильный машинный код из каталога §5; фронт показывает текст по коду.
- `message` — английский текст для логов и отладки, не для UI.
- `params` — значения для подстановки в локализованный текст.
- `details` — необязательный список вложенных ошибок того же вида (например,
  ошибки валидации по полям: `params.field`).

### 1.5 Коды выхода процесса

| Код | Значение |
|-----|----------|
| 0 | Успех (`ok: true`) |
| 1 | Ошибка бизнес-логики / удалённой стороны (`ok: false`) |
| 2 | Неверный вызов: неизвестная команда, флаги, невалидный JSON (`ok: false`, `code: request.*`) |
| 3 | Хранилище не готово: нет БД/ключ-файла, ключ не подходит (`store.*`, `crypto.*`) |
| 4 | Несовместимая версия протокола или схемы БД (`protocol.*`, `store.schema_too_new`) |
| 70 | Внутренняя ошибка (`internal`) |

Фронт ориентируется на JSON, код выхода — для скриптов и быстрой проверки.
Пачечные операции (`conn check` по многим) возвращают 0, даже если отдельные
серверы недоступны: результат каждого — в `items`.

### 1.6 Версия протокола и рукопожатие

- `protocol` — целое число, растёт только при **несовместимых** изменениях.
  Добавление команд, полей и кодов ошибок совместимо и версию не меняет;
  фронт обязан игнорировать неизвестные поля и показывать неизвестный код
  ошибки как общий текст + `message`.
- Фронт при старте вызывает `version`, при работе с демоном — `hello` (§4.3).

```
sherpa-core version [--require-protocol 1]
```

```json
{ "ok": true, "protocol": 1, "data": {
  "core_version": "0.1.0",
  "protocol": 1,
  "protocol_min": 1,
  "schema_version": 3,
  "build": { "target": "x86_64-unknown-linux-gnu", "commit": "5af0046" },
  "paths": { "config": "...", "db": "...", "key_file": "..." },
  "initialized": true
}}
```

С `--require-protocol N`: если N вне `[protocol_min, protocol]` —
ошибка `protocol.unsupported`, выход 4.

---

## 2. Передача секретов

- Секреты (пароль, текст ключа, парольная фраза) **никогда** не передаются
  в аргументах и переменных окружения — их видно в `ps` и `/proc`.
- Они передаются в JSON через stdin (`--input -`); ядро отклоняет флаги
  `--password`/`--key-text` и т. п. — таких флагов нет.
- Ядро **не возвращает** секреты: в ответах вместо них признаки
  `has_password`, `has_key_text`, `has_passphrase`.
- Изменение секрета в `update`: поле есть со строкой — заменить; `null` —
  удалить; поля нет — не трогать.
- Ключ-файл при `keyfile export` пишется ядром прямо в файл назначения
  (права 600), в stdout не выводится.
- Ответы на интерактивные запросы при подключении (host key, повтор пароля)
  в демоне идут через сокет, доступный только текущему пользователю.

Пример (Rust/C# фронт пишет в stdin процесса):

```
echo '{"name":"web-1","host":"10.0.0.5","user":"root",
       "auth":{"method":"password","password":"s3cr3t"}}' \
  | sherpa-core conn create --input -
```

---

## 3. Команды MVP

Сводка:

| Команда | Назначение |
|---------|------------|
| `version` | Версия, протокол, пути |
| `init` | Создать БД, ключ-файл, конфиг |
| `keyfile export` / `keyfile status` | Резервная копия ключ-файла / проверка |
| `conn create/get/list/update/delete/duplicate` | Подключения |
| `group create/list/update/delete` | Группы |
| `tag list/rename/delete` | Теги |
| `conn check` | Проверка одного или пачки |
| `status list` | Статусы из БД без проверки |
| `hostkey list/get/accept/reject/forget` | Host keys (TOFU) |
| `import ssh-config preview/apply` | Импорт `~/.ssh/config` |
| `daemon` / `daemon ensure` / `daemon status` / `daemon stop` | Демон терминала |

Любая команда, кроме `version` и `init`, на неинициализированном хранилище
возвращает `store.not_initialized` (выход 3). Миграции схемы БД применяются
автоматически при первом обращении новой версии ядра.

### 3.1 Инициализация

```
sherpa-core init [--force-new-key]
```

Создаёт (если нет) конфиг, БД (с миграциями) и ключ-файл (256 бит, права 600).
Идемпотентна: если всё уже есть и ключ подходит к БД — ничего не меняет.

```json
{ "ok": true, "protocol": 1, "data": {
  "created": { "config": false, "db": true, "key_file": true },
  "paths": { "config": "...", "db": "...", "key_file": "..." },
  "key_fingerprint": "SHA256:Qm9v...",
  "backup_recommended": true
}}
```

Ошибки: `store.key_file_missing` — БД с секретами есть, а ключ-файла нет
(новый ключ не создаётся молча; `--force-new-key` создаёт новый, все
сохранённые секреты удаляются — фронт обязан спросить подтверждение);
`crypto.key_mismatch` — ключ-файл не подходит к БД; `store.permission_denied`.

`backup_recommended: true` — ключ создан только что или его копия ещё не
экспортировалась; фронт предлагает сохранить копию.

### 3.2 Ключ-файл

```
sherpa-core keyfile export --to <path> [--overwrite]
```

```json
{ "ok": true, "protocol": 1, "data": {
  "path": "/media/usb/sherpa.key", "key_fingerprint": "SHA256:Qm9v...",
  "exported_at": "2026-10-24T09:15:00Z" } }
```

Ошибки: `fs.exists`, `fs.write_failed`, `store.key_file_missing`.

```
sherpa-core keyfile status
```

```json
{ "ok": true, "protocol": 1, "data": {
  "path": "...", "present": true, "matches_db": true,
  "permissions_ok": true, "key_fingerprint": "SHA256:Qm9v...",
  "last_export_at": null } }
```

Импорт/замена ключ-файла отдельной командой не делается: восстановление =
положить файл по пути из конфига (или указать `--key-file`).

### 3.3 Подключения

Объект `Connection` в ответах — см. §6.1.

#### `conn create`

```
sherpa-core conn create --input -
```

```json
{
  "name": "web-1",
  "host": "10.0.0.5",
  "port": 22,
  "user": "root",
  "auth": { "method": "key_text", "key_text": "-----BEGIN OPENSSH PRIVATE KEY-----\n...", "passphrase": "pp" },
  "description": "Фронт прода",
  "group_id": "0192f0c4-0000-7000-8000-000000000001",
  "tags": ["prod", "nginx"]
}
```

`auth` — один из вариантов:

| `method` | Поля |
|----------|------|
| `password` | `password` (необяз.: без него пароль спросят при подключении) |
| `key_file` | `key_path`, `passphrase` (необяз.) |
| `key_text` | `key_text`, `passphrase` (необяз.) |

Простые поля можно передать флагами: `--name --host --port --user
--description --group-id --tag` (повторяемый), `--auth key_file --key-path`.

Ответ: `{"connection": Connection}`.
Валидация: имя непустое, ≤ 128 символов, **уникально** (без учёта регистра);
хост непустой; порт 1–65535; ключ из `key_text` разбирается сразу
(`auth.key_invalid`), `key_file` на существование не проверяется (может быть
на внешнем диске) — проблема выявится в `check`. Теги нормализуются:
обрезка пробелов, нижний регистр, без дублей; несуществующие создаются.

#### `conn get`

```
sherpa-core conn get <id>
```

`{"connection": Connection}`; ошибка `conn.not_found`.

#### `conn list`

```
sherpa-core conn list [--q <text>] [--host <text>] [--tag <t>]... [--group-id <id>|--no-group]
                      [--recursive] [--status online|offline|auth_failed|error|unknown]
                      [--sort name|host|status|last_checked] [--desc]
                      [--limit N] [--offset N]
```

- `--q` — подстрока (без учёта регистра) в имени **или** хосте.
- `--host` — подстрока только в хосте.
- `--tag` — повторяемый, логика **И** (у подключения есть все теги).
- `--group-id` — в группе; `--recursive` — с подгруппами; `--no-group` — без группы.
- Без `--limit` — все (масштаб ≤ 100, пагинация на будущее).

```json
{ "ok": true, "protocol": 1, "data": {
  "items": [ { "id": "...", "name": "web-1", "host": "10.0.0.5", "port": 22,
               "user": "root", "auth": { "method": "password", "has_password": true },
               "group_id": null, "tags": ["prod"],
               "status": { "state": "online", "checked_at": "2026-10-24T09:00:00Z" } } ],
  "total": 1
}}
```

#### `conn update`

```
sherpa-core conn update <id> --input -
```

Частичное обновление (merge patch): переданные поля меняются, `null`
очищает необязательное поле (`description`, `group_id`, секреты).
`tags` заменяет набор целиком. Смена `auth.method` требует новые данные
метода; старые секреты другого метода удаляются.

```json
{ "port": 2222, "tags": ["prod"], "auth": { "method": "password", "password": "new" } }
```

Ответ: `{"connection": Connection}`. Смена `host`/`port` сбрасывает статус
в `unknown` (host key привязан к host:port, а не к подключению).

#### `conn delete`

```
sherpa-core conn delete <id>...
```

```json
{ "ok": true, "protocol": 1, "data": { "deleted": ["..."], "not_found": [] } }
```

Секреты удаляются вместе с подключением. Открытые терминальные сессии этого
подключения в демоне не рвутся.

#### `conn duplicate`

```
sherpa-core conn duplicate <id> [--name <new>]
```

Копирует все поля, включая секреты и теги. Имя по умолчанию —
`"<name> (copy)"`, `"<name> (copy 2)"`… (суффикс не локализуется; фронт
может сразу передать своё `--name`). Статус копии — `unknown`.
Ответ: `{"connection": Connection}`.

### 3.4 Группы

Группы — дерево папок (вложенность разрешена). `Group` — §6.2.

```
sherpa-core group create --name <n> [--parent-id <id>]
sherpa-core group list
sherpa-core group update <id> [--name <n>] [--parent-id <id>|--root]
sherpa-core group delete <id> [--mode move_to_parent|delete_connections]
```

- Имя уникально среди соседей (`group.name_taken`).
- Перенос в собственного потомка — `group.cycle`.
- `delete` по умолчанию `move_to_parent`: подключения и подгруппы
  переносятся в родителя (или в корень). `delete_connections` удаляет
  поддерево вместе с подключениями.

`group list`:

```json
{ "ok": true, "protocol": 1, "data": { "items": [
  { "id": "g1", "name": "Prod", "parent_id": null, "connection_count": 12 },
  { "id": "g2", "name": "DB",   "parent_id": "g1", "connection_count": 3 }
]}}
```

### 3.5 Теги

Теги создаются неявно через подключения; пустые теги удаляются автоматически.

```
sherpa-core tag list
sherpa-core tag rename <old> <new>     # если <new> уже есть — слияние
sherpa-core tag delete <tag>           # снять со всех подключений
```

```json
{ "ok": true, "protocol": 1, "data": { "items": [
  { "name": "prod", "connection_count": 12 }, { "name": "nginx", "connection_count": 4 } ] } }
```

### 3.6 Проверка подключения

```
sherpa-core conn check <id>... | --all | --group-id <id> [--recursive] | --tag <t>...
                       [--timeout <sec>] [--concurrency N] [--input -]
```

- Одна или пачка до **100** подключений (больше — `request.too_many`),
  параллельно (по умолчанию `check_concurrency` = 32), таймаут на сервер
  `--timeout` (по умолчанию из конфига, 10 с).
- Проверка: TCP → SSH handshake → сверка host key → авторизация → закрытие.
  Команд на сервере не выполняет.
- Результат пишется в БД как статус.
- Для `password` без сохранённого пароля, или ключа с несохранённой
  парольной фразой, разовые секреты можно передать в stdin:
  `{"secrets": {"<id>": {"password": "..."}}}`; иначе результат
  `auth.secret_required`.
- Если host key **неизвестен** — по TOFU он не принимается молча при
  пачечной проверке: результат `hostkey.unknown` с отпечатком, фронт
  показывает его и вызывает `hostkey accept`. Флаг `--accept-new` принимает
  новые (не изменённые!) ключи автоматически — для одиночной проверки
  после подтверждения пользователем или массового первого импорта.
- Изменённый host key — всегда `hostkey.mismatch`, авторизация не
  выполняется.

```json
{ "ok": true, "protocol": 1, "data": {
  "items": [
    { "id": "c1", "state": "online", "checked_at": "2026-10-24T09:15:02Z", "latency_ms": 41,
      "server_banner": "SSH-2.0-OpenSSH_9.6p1", "error": null },
    { "id": "c2", "state": "auth_failed", "checked_at": "...", "latency_ms": 60,
      "error": { "code": "auth.rejected", "message": "Authentication rejected", "params": { "methods": ["publickey"] } } },
    { "id": "c3", "state": "error", "checked_at": "...",
      "error": { "code": "hostkey.unknown", "message": "Unknown host key",
                 "params": { "host": "10.0.0.7", "port": 22, "key_type": "ssh-ed25519",
                             "fingerprint": "SHA256:abc..." } } },
    { "id": "c4", "state": "offline", "checked_at": "...",
      "error": { "code": "net.timeout", "message": "Connection timed out", "params": { "timeout_sec": 10 } } }
  ],
  "summary": { "total": 4, "online": 1, "offline": 1, "auth_failed": 1, "error": 1 }
}}
```

Соответствие ошибки и статуса:

| `state` | Когда |
|---------|-------|
| `online` | Авторизация успешна |
| `offline` | `net.*` (DNS, отказ, таймаут, недоступна сеть) |
| `auth_failed` | `auth.*` (отказ, неверный ключ/фраза, нет секрета) |
| `error` | `hostkey.*`, `ssh.*`, `fs.*` (нет файла ключа) и прочее |
| `unknown` | Не проверялся (только в `status list`, не в `check`) |

Если `user` у подключения пуст, используется имя текущего пользователя ОС
(как в OpenSSH). Разовые секреты в `secrets` — `password` и/или `passphrase`.

**`--stream`** — живой прогресс. Вместо одного объекта ядро пишет JSON Lines:
строку на каждый сервер сразу по готовности (порядок — по завершению, не по
входу), последней строкой — обычный конверт §1.3 с итогом, без `items`:

```
{"event":"result","item":{"id":"c4","state":"offline","checked_at":"...","latency_ms":null,"server_banner":null,"error":{"code":"net.refused",...}}}
{"event":"result","item":{"id":"c1","state":"online","checked_at":"...","latency_ms":41,"server_banner":"SSH-2.0-OpenSSH_9.6p1","error":null}}
{"ok":true,"protocol":1,"data":{"summary":{"online":1,"offline":1,"auth_failed":0,"error":0,"total":2}}}
```

Ошибка всего вызова (нет хранилища, неверные аргументы) — одна строка-конверт
с `ok: false`, как обычно; признак конца потока — строка с полем `ok`.

### 3.7 Статусы

```
sherpa-core status list [<id>...]
```

Читает сохранённые статусы без сети:

```json
{ "ok": true, "protocol": 1, "data": { "items": [
  { "id": "c1", "state": "online", "checked_at": "2026-10-24T09:15:02Z", "error": null },
  { "id": "c5", "state": "unknown", "checked_at": null, "error": null } ] } }
```

Терминальное подключение через демон тоже обновляет статус.

### 3.8 Host keys (TOFU)

Ключи хранятся по `(host, port)` — общий для всех подключений к этому
адресу. На один адрес — по одному ключу каждого типа, который сервер
предъявил. `HostKey` — §6.4.

```
sherpa-core hostkey list [--host <h>] [--port <p>]
sherpa-core hostkey get --host <h> [--port 22]
sherpa-core hostkey accept --host <h> [--port 22] --key-type <t> --fingerprint <SHA256:...> [--replace]
sherpa-core hostkey reject --host <h> [--port 22] --fingerprint <SHA256:...>
sherpa-core hostkey forget --host <h> [--port 22] [--key-type <t>]
```

- `accept` — принять отпечаток, полученный из `hostkey.unknown` /
  `hostkey.mismatch`. Ядро **само заново** получает ключ с сервера и
  сохраняет его, только если отпечаток совпал с переданным (иначе
  `hostkey.fingerprint_changed`) — фронт не передаёт сам ключ, и подмена
  между показом и принятием исключена.
- Если для адреса уже есть другой ключ этого типа — нужен `--replace`
  (это и есть «сменить ключ»), иначе `hostkey.mismatch`.
- `reject` — запомнить отклонённый отпечаток: следующие проверки сразу
  дают `hostkey.rejected` без повторного вопроса, пока не будет `accept`
  или `forget`.
- `forget` — удалить сохранённый ключ(и) адреса; при следующем подключении
  снова TOFU.

Ответ `accept`:

```json
{ "ok": true, "protocol": 1, "data": { "host_key": {
  "host": "10.0.0.7", "port": 22, "key_type": "ssh-ed25519",
  "fingerprint": "SHA256:abc...", "state": "trusted",
  "first_seen_at": "...", "accepted_at": "...", "replaced_fingerprint": null } } }
```

Параметры ошибки `hostkey.mismatch` (для предупреждения):
`host, port, key_type, fingerprint` (новый), `known_fingerprint`,
`known_accepted_at`.

### 3.9 Импорт `~/.ssh/config`

Двухшаговый: `preview` ничего не пишет, `apply` создаёт выбранное.

```
sherpa-core import ssh-config preview [--path <file>] [--group-id <id>]
```

`--path` по умолчанию — `~/.ssh/config` (Windows: `%USERPROFILE%\.ssh\config`).

Правила разбора (ТЗ §4.4):

- Читаются `Host`, `HostName`, `Port`, `User`, `IdentityFile`, `Include`;
  прочие директивы пропускаются и перечисляются в `ignored_directives`.
- `Include` раскрывается рекурсивно (глоб, относительные пути — от
  `~/.ssh`), глубина ≤ 16, циклы обнаруживаются (`import.include_cycle`
  в `warnings`, не фатально).
- `Host` с шаблоном (`*`, `?`, `!`) не становится сервером; его значения
  применяются к подходящим конкретным хостам по правилу OpenSSH «первое
  найденное значение побеждает» (порядок в файле). `Match` не
  поддерживается — блок пропускается с предупреждением.
- В строке `Host a b c` каждый конкретный алиас — отдельный кандидат.
- Имя подключения = алиас `Host`; хост = `HostName` (с `%h` → алиас), если
  нет — алиас; порт по умолчанию 22; пользователь — если нет, пусто
  (решение на подтверждение: брать текущего пользователя ОС?).
- `IdentityFile` → `auth: key_file` с путём (раскрываются `~`, `%d`, `%u`,
  `%h`, `%r`); ключ не копируется. Несколько `IdentityFile` — берётся
  первый, в `warnings` отмечается. Без `IdentityFile` — `auth: password`
  без сохранённого пароля.
- Дубликат: имя уже есть в БД (без учёта регистра) → `action: "skip"`,
  `reason: "name_exists"`. Повтор алиаса внутри файлов → второй `skip`,
  `reason: "duplicate_in_file"`.

Ответ preview:

```json
{ "ok": true, "protocol": 1, "data": {
  "preview_id": "3f9a0c5e1b7d42a8c6e0f1d2b3a49c8e",
  "source": "/home/u/.ssh/config",
  "files": ["/home/u/.ssh/config", "/home/u/.ssh/config.d/work"],
  "items": [
    { "key": "web-1", "action": "create",
      "connection": { "name": "web-1", "host": "10.0.0.5", "port": 22, "user": "deploy",
                      "auth": { "method": "key_file", "key_path": "/home/u/.ssh/id_ed25519" },
                      "group_id": null, "tags": [] },
      "origin": { "file": "/home/u/.ssh/config", "line": 12 },
      "defaults_from": ["Host *"], "warnings": [] },
    { "key": "db-1", "action": "skip", "reason": "name_exists",
      "existing_id": "c7", "origin": { "file": "...", "line": 30 } }
  ],
  "ignored_directives": [ { "directive": "ProxyJump", "file": "...", "line": 14 } ],
  "warnings": [ { "code": "import.match_unsupported", "params": { "file": "...", "line": 40 } } ]
}}
```

```
sherpa-core import ssh-config apply --input -
```

```json
{ "path": "/home/u/.ssh/config",
  "select": ["web-1", "web-2"],
  "group_id": "g1",
  "tags": ["imported"],
  "overrides": { "web-2": { "name": "web-2-old" } } }
```

`apply` **заново** разбирает файл (предпросмотр не хранится в БД; `preview_id`
— для сверки: если файл изменился, `import.source_changed`, фронт делает
повторный preview). `preview_id` — непрозрачная строка (отпечаток содержимого
всех прочитанных файлов). Импортируются только ключи из `select` с
`action: create`; без `select` — все новые. `overrides` позволяет
переименовать пропущенный дубликат. Ключ из `select`, которого нет в файле,
попадает в `failed` с кодом `import.unknown_key`. Флагами: `--path`,
`--preview-id`, `--select` (повторяемый), `--group-id`, `--tag`.

```json
{ "ok": true, "protocol": 1, "data": {
  "created": [ { "key": "web-1", "id": "c9" } ],
  "skipped": [ { "key": "db-1", "reason": "name_exists" },
               { "key": "web-3", "reason": "not_selected" } ],
  "failed":  [ { "key": "web-2", "error": { "code": "validation.invalid", "message": "...", "params": { "field": "port" } } } ]
}}
```

Импорт идёт одной транзакцией по созданным; ошибки отдельных записей
попадают в `failed` и остальным не мешают.

---

## 4. Демон

Демон держит долгие операции: терминальные сессии, соединения рабочего
стола, подписки утилит. Разовые команды §3 работают без него.

### 4.1 Запуск и обнаружение

```
sherpa-core daemon ensure          # запустить, если не работает (ждёт ≤ 5 с)
sherpa-core daemon status          # {running, pid, endpoint, sessions, clients, connections, uptime_sec, …}
sherpa-core daemon stop [--force]  # без --force — отказ daemon.sessions_active при сессиях
sherpa-core daemon run             # сам демон (так его запускает ensure; `daemon` без подкоманды — то же)
```

Ответ `ensure`:

```json
{ "ok": true, "protocol": 1, "data": {
  "endpoint": "/run/user/1000/sherpa/daemon.sock", "pid": 4242,
  "started": true, "core_version": "0.1.0", "protocol": 1 } }
```

- `ensure` запускает тот же бинарник отсоединённым процессом
  (`--config/--db/--key-file` передаются явно), stderr демона пишется в
  `daemon.log` рядом с lock-файлом. Если работает демон другой версии и у
  него нет сессий — он перезапускается; с сессиями — остаётся старый.
- Адрес:
  - Linux: `$XDG_RUNTIME_DIR/sherpa/daemon.sock` (каталог 700, сокет 600);
    без `XDG_RUNTIME_DIR` — `~/.local/state/sherpa/daemon.sock`.
  - Windows: named pipe `\\.\pipe\sherpa-core-<пользователь>`, DACL — только
    владелец и SYSTEM, удалённые клиенты отклоняются.
  - Нестандартный `--db` → к имени добавляется `-<8 hex>` от пути БД: у
    разных хранилищ разные демоны.
- Один демон на хранилище: эксклюзивный lock-файл `daemon.lock` (в нём pid);
  второй экземпляр — `daemon.already_running` с `pid`. Старый файл сокета
  удаляет новый демон после захвата lock-файла.
- Авто-выход: нет сессий и нет подключённых клиентов дольше
  `daemon_idle_timeout_sec` (по умолчанию 300, `0` — не выходить). Любой
  клиент (в т. ч. `daemon status`) продлевает жизнь.
- Перед выходом демон шлёт клиентам `daemon_shutdown`, закрывает сессии
  (`closed`, `reason: client`) и соединения.

### 4.2 Протокол поверх сокета

JSON Lines: объект на строку, `\n`, UTF-8, строка ≤ 1 МиБ.

| Вид | Направление | Форма |
|-----|-------------|-------|
| Запрос | клиент → демон | `{"id": 7, "method": "...", "params": {...}}` |
| Ответ | демон → клиент | `{"id": 7, "ok": true, "data": ...}` / `{"id": 7, "ok": false, "error": {...}}` |
| Событие | демон → клиент | `{"event": "...", ...}` (без `id`) |

- `id` — любое JSON-значение, возвращается в ответе как есть. Запросы
  выполняются параллельно; порядок ответов не гарантирован — сверяйте `id`.
- Первым должен быть `hello`, иначе `protocol.hello_required`.
- `error` — формат §1.4. Невалидная строка — ответ с `id: null` и
  `request.invalid_json`.
- Бинарные данные — base64 (стандартный, с паддингом) в `data_b64`.
- Исходящая очередь клиента ограничена (32 сообщения ≈ до 1,4 МиБ вывода).
  Пока клиент не читает, демон не читает SSH-каналы его сессий
  (backpressure): данные не теряются, сервер притормаживается окном SSH.

Методы сгруппированы по префиксу (`session.*`, `desktop.*`, `monitor.*`…);
список — `daemon.methods`. Новая утилита добавляет свою группу (см.
`docs/UTILITY_GUIDE.md`).

### 4.3 Служебные методы

| Метод | Параметры | Ответ |
|-------|-----------|-------|
| `hello` | `protocol` (обяз.), `client` | `{protocol, core_version, daemon_pid, sessions: [id]}` |
| `ping` | — | `{pong: true}` |
| `daemon.status` | — | как `sherpa-core daemon status` |
| `daemon.stop` | `force` | `{stopping: true, sessions}`; демон выходит через ~100 мс |
| `daemon.methods` | — | `{items: ["hello", "ping", "session.open", …]}` |

```json
{"id":1,"method":"hello","params":{"protocol":1,"client":"sherpa-qt/0.1.0"}}
{"id":1,"ok":true,"data":{"protocol":1,"core_version":"0.1.0","daemon_pid":4242,"sessions":["s1"]}}
```

### 4.4 Терминал: `session.*`

| Метод | Параметры | Ответ |
|-------|-----------|-------|
| `session.open` | `connection_id`, `cols` (80), `rows` (24), `term` (`xterm-256color`), `env` `{k: v}`, `secrets` `{password, passphrase}` | `{session}` сразу; дальше — события |
| `session.input` | `session`, `data_b64` | `null` |
| `session.resize` | `session`, `cols`, `rows` (1–10000) | `null` |
| `session.close` | `session` | `null`; затем `closed` с `reason: client` |
| `session.list` | — | `{items: [Session §6.6]}` |
| `session.attach` | `session` | `{session, replay_bytes}`; затем `output` с `replay: true` (хвост до 64 КиБ) и неотвеченный вопрос, если был |
| `session.hostkey_answer` | `prompt_id`, `decision`: `accept` \| `accept_once` \| `reject` \| `cancel`, `session` (необяз.) | `null` |
| `session.auth_answer` | `prompt_id`, `responses: [строка]`, `save` (bool) **или** `cancel: true`; `session` (необяз.) | `null` |

- Сессия принадлежит клиенту, который её открыл (или подхватил `attach`).
  Чужая сессия для `input/resize/close` — `session.not_found`.
- Клиент отключился → сессия «отцеплена» и ждёт `session.attach` 30 с, затем
  закрывается (`reason: client`). Вывод за это время копится в хвосте 64 КиБ.
- Терминал идёт поверх общего соединения пула (§4.6): второй терминал к тому
  же серверу не открывает новое SSH-соединение.
- Вывод склеивается: событие не чаще чем раз в ~10 мс, не больше 32 КиБ.
- `decision`: `accept` — сохранить ключ (при смене — заменить) и продолжить;
  `accept_once` — продолжить без сохранения; `reject` — запомнить отклонение
  (`hostkey reject`) и закрыть; `cancel` — закрыть.
- `auth_prompt` приходит, если пароль/фраза не сохранены или отвергнуты
  (до 3 попыток). `save: true` — сохранить введённое в подключение.
  `cancel: true` → ошибка `auth.cancelled`.
- Неотвеченный вопрос ждёт 300 с, затем считается отменой. Ответ на уже
  неактуальный вопрос — `session.prompt_expired`.
- Успешное подключение и ошибка подключения обновляют статус (§3.7).

### 4.5 События

| `event` | Поля | Когда |
|---------|------|-------|
| `connecting` | `session, connection_id` | Начато подключение |
| `hostkey_prompt` | `session?, connection_id, prompt_id, kind: unknown\|mismatch, host, port, key_type, fingerprint, known_fingerprint?` | Нужно решение TOFU |
| `auth_prompt` | `session?, connection_id, prompt_id, kind: password\|passphrase, prompts: [{text, echo}], attempt` | Нужен секрет |
| `connected` | `session, connection_id, server_banner` | PTY открыт, шелл запущен |
| `output` | `session, data_b64, replay?` | Вывод PTY (stdout+stderr) |
| `closed` | `session, reason: exit\|client\|remote\|error, exit_status?, signal?` | Последнее событие сессии |
| `error` | `session?` или `subscription?`, `error`, `fatal` | Ошибка; при `fatal` для сессии следом `closed` |
| `monitor` | `subscription, connection_id, data` | Очередной замер (§4.8) |
| `daemon_shutdown` | `reason: idle\|stop` | Демон завершается |

У вопросов от `desktop.open` нет поля `session` — они приходят клиенту,
вызвавшему `desktop.open`; отвечать теми же `session.*_answer` без `session`.

```
→ {"id":2,"method":"session.open","params":{"connection_id":"c1","cols":120,"rows":34}}
← {"id":2,"ok":true,"data":{"session":"s1"}}
← {"event":"connecting","session":"s1","connection_id":"c1"}
← {"event":"hostkey_prompt","session":"s1","connection_id":"c1","prompt_id":"p3","kind":"unknown","host":"10.0.0.5","port":22,"key_type":"ssh-ed25519","fingerprint":"SHA256:abc..."}
→ {"id":3,"method":"session.hostkey_answer","params":{"session":"s1","prompt_id":"p3","decision":"accept"}}
← {"id":3,"ok":true,"data":null}
← {"event":"connected","session":"s1","connection_id":"c1","server_banner":"SSH-2.0-OpenSSH_9.6p1"}
← {"event":"output","session":"s1","data_b64":"V2VsY29tZS4uLg0K"}
→ {"id":4,"method":"session.input","params":{"session":"s1","data_b64":"ZXhpdA0="}}
← {"event":"closed","session":"s1","reason":"exit","exit_status":0}
```

### 4.6 Рабочий стол: соединения и возможности — `desktop.*`

Одно SSH-соединение на подключение («пул»); внутри — каналы терминалов и
утилит. Утилиты ограничены 8 одновременными каналами `exec`/SFTP/TCP на соединение
(остальные ждут). Соединение без терминалов и каналов закрывается после
10 мин простоя.

| Метод | Параметры | Ответ |
|-------|-----------|-------|
| `desktop.open` | `connection_id`, `secrets?` | `{connection, capabilities}`; вопросы host key/пароля — событиями этому клиенту |
| `desktop.capabilities` | `connection_id`, `refresh` | Capabilities (кэш соединения) |
| `desktop.list` | — | `{items: [{connection_id, opened_at, alive, terminals, channels_in_use, readonly, server_banner}]}` |
| `desktop.close` | `connection_id` | `{closed: bool}` |

Утилиты (`monitor.*`, `priv.*`…) используют соединение пула; если его нет,
подключаются **без вопросов** (сохранённые ключи и секреты) — при
неизвестном host key или нужном пароле вернут ошибку (`hostkey.unknown`,
`auth.secret_required`). Поэтому рабочий стол сначала вызывает `desktop.open`.

Capabilities:

```json
{ "os": { "id": "debian", "version": "12", "name": "Debian GNU/Linux 12 (bookworm)",
          "kernel": "Linux", "kernel_release": "6.1.0-18-amd64", "arch": "x86_64" },
  "user": { "name": "deploy", "uid": 1000, "is_root": false },
  "privileges": { "sudo": true, "sudo_nopasswd": false },
  "systemd": true, "procfs": true, "package_manager": "apt",
  "tools": { "sudo": true, "systemctl": true, "journalctl": true, "ss": true, "docker": false, "…": false },
  "utilities": { "terminal": true, "monitor": true, "services": true, "logs": true, "network": true,
                 "firewall": false, "disks": true, "packages": true, "cron": true, "docker": false } }
```

`utilities` — что фронт может показать; окна, для которых `false`, скрываются
или отключаются.

### 4.7 Привилегии — `priv.*`

| Метод | Параметры | Ответ |
|-------|-----------|-------|
| `priv.sudo.check` | `connection_id` | PrivStatus |
| `priv.sudo.auth` | `connection_id`, `password` | PrivStatus; неверный пароль — `priv.auth_failed` |
| `priv.sudo.forget` | `connection_id` | PrivStatus |
| `priv.readonly.set` | `connection_id`, `readonly` | PrivStatus |

PrivStatus: `{connection_id, is_root, sudo_available, sudo_nopasswd,
sudo_authenticated, readonly}`.

- Пароль sudo проверяется (`sudo -S -k -v`) и хранится только в памяти
  демона до закрытия соединения; не пишется ни в БД, ни в логи.
- Команды с правами root утилиты выполняют так: root — напрямую;
  NOPASSWD — `sudo -n`; есть пароль — `sudo -S` (пароль в stdin); иначе
  `priv.required` (фронт показывает баннер и вызывает `priv.sudo.auth`).
- `readonly: true` — любые изменения на сервере через утилиты отклоняются
  (`priv.readonly`); чтение работает. Терминал не ограничивается.

### 4.8 Утилита «Монитор» — `monitor.*`

| Метод | Параметры | Ответ |
|-------|-----------|-------|
| `monitor.snapshot` | `connection_id`, `processes` (50, ≤ 500), `sort`: `cpu`\|`mem` | MonitorData (≈ 0,5 с: два замера CPU) |
| `monitor.subscribe` | `connection_id`, `interval_ms` (2000, ≥ 1000), `processes`, `sort` | `{subscription, interval_ms}`; затем события `monitor` |
| `monitor.unsubscribe` | `subscription` | `{unsubscribed}`; чужая/неизвестная — `subscription.not_found` |

Подписки клиента отменяются при его отключении. Ошибка замера — событие
`error` с `subscription`, `fatal: true`, подписка завершается.

MonitorData:

```json
{ "timestamp": "2026-10-24T09:15:02Z",
  "cpu": { "usage_pct": 23.5, "cores": 4, "per_core": [30.1, 12.0, 40.2, 11.7] },
  "memory": { "total_bytes": 8192000000, "used_bytes": 2048000000, "available_bytes": 6144000000,
              "free_bytes": 1024000000, "buffers_bytes": 102400000, "cached_bytes": 2252800000, "usage_pct": 25.0 },
  "swap": { "total_bytes": 2048000000, "used_bytes": 512000000, "usage_pct": 25.0 },
  "load": { "1m": 0.52, "5m": 0.40, "15m": 0.31, "running": 2, "total": 345 },
  "uptime_sec": 86461,
  "processes": [ { "pid": 812, "ppid": 1, "user": "mysql", "cpu_pct": 12.5, "mem_pct": 4.1,
                   "rss_bytes": 343040000, "elapsed_sec": 86400, "state": "Ssl", "command": "mysqld" } ] }
```

`usage_pct` — `null`, если нет второго замера. Источник — `/proc` и `ps`
(procps); на сервере без `/proc` — `cap.missing` с `capability: procfs`.

---

### 4.9 Docker, веб-серверы и базы данных — `docker.*`, `web.*`, `db.*`

Все вызовы требуют `connection_id`, кроме `docker.unsubscribe`.
Изменяющие вызовы проверяют `readonly`, требуют **`confirm: true`** и
одноразовый **`confirm_token`** из `danger.prepare` (§4, общие подтверждения);
фронт передаёт его после диалога с перечнем объектов и именем сервера
(для остановки на Prod — после ввода имени сервера).
Docker и веб-команды выполняются через `priv.*` (root/sudo).
БД используют пользователя SSH и штатную клиентскую авторизацию на сервере:
`~/.my.cnf`, PostgreSQL peer/`.pgpass`, Redis default local socket/port.
Пароли в параметрах и аргументах процессов не принимаются.

| Метод | Дополнительные параметры | Ответ |
|-------|---------------------------|-------|
| `docker.ping` | — | `{items: [DockerVersion]}` |
| `docker.ps` | — | `{items: [DockerContainer]}` (включая остановленные) |
| `docker.images`, `docker.volumes`, `docker.networks` | — | `{items: [DockerObject]}` |
| `docker.inspect` | `id` | `{items: [DockerInspect]}` |
| `docker.start`, `docker.stop`, `docker.restart`, `docker.pause`, `docker.rm` | `id`, `confirm: true` | `{completed: true}` |
| `docker.logs` | `id`, `tail` (200, ≤5000), `follow` (false) | `{lines: [строка]}` или `{subscription, interval_ms}` |
| `docker.stats` | `subscribe` (false), `interval_ms` (2000, ≥1000) | `{items: [DockerStats]}` или `{subscription, interval_ms}` |
| `docker.unsubscribe` | `subscription` | `{unsubscribed}` |
| `web.detect` | — | `{tools: [nginx\|apachectl\|certbot\|openssl]}` |
| `web.sites.list` | `engine`: `nginx` (default)\|`apache` | `{items: [WebSite]}` |
| `web.sites.get` | `engine`, `path` внутри `/etc/nginx/` или `/etc/apache2/` | `{path, content}` |
| `web.test` | `engine` | `{valid: bool, lines: [строка]}`; неуспешная проверка — `valid: false` |
| `web.reload` | `engine`, `confirm: true` | `{completed: true}`; `nginx -t`/`apachectl configtest` выполняется **заново в том же скрипте**, reload только при коде 0 |
| `web.certs.list` | — | `{items: [WebCertificate]}`: certbot + пути сертификатов из конфигов, даты openssl |
| `web.certs.renew` | `dry_run` (false), `confirm: true` | `{completed: true}`; certbot non-interactive |
| `db.detect` | — | `{tools: [mysql\|mariadb\|psql\|redis-cli\|mysqldump\|mariadb-dump\|pg_dump]}` |
| `db.list` | `engine`: `mysql`\|`mariadb`\|`postgres`\|`redis` | `{items: [Database]}` |
| `db.queries` | `engine` | `{items: [DatabaseQuery]}`; Redis: поля `CLIENT LIST` |
| `db.kill` | `engine`, `id` (положительное целое), `confirm: true` | `{completed: true}`; MySQL KILL QUERY, PostgreSQL pg_cancel_backend, Redis CLIENT KILL |
| `db.dump` | `engine`, `database`, `path` (файл в `/var/backups/`), `confirm: true` | `{completed: true, path}` после записи; MySQL SQL, PostgreSQL custom, Redis полный RDB экземпляра |

DockerObject/Container/Version — объекты штатного Docker CLI JSON (`--format json`),
имена полей CLI сохранены. Inspect ограничен `Id, Name, Image, State, Mounts,
NetworkSettings`: переменные окружения, labels и аргументы команд не выдаются.
DockerStats: `{id, name, cpu_pct, memory_used_bytes, memory_limit_bytes}`.

Поток логов: события `docker.logs {subscription, stderr, data_b64}` (base64
фрагментов stdout/stderr), затем `docker.logs.done {subscription}` при EOF.
Это SSH-поток с ограниченной очередью, без опроса и повторения хвоста.
Статистика: `docker.stats {subscription, data: {items: [DockerStats]}}`.
Ошибка подписки: `error {subscription, error, fatal: true}`. Подписки отменяются
при отключении клиента; чужая подписка недоступна для отмены.

WebSite nginx: `{path, names: [строка], root: строка|null, listen: [строка], ssl: bool}`;
Apache: `{names: [строка], source: строка}` из `apachectl -S`.
WebCertificate: `{name?, domains?, path, expires_at, subject?, source?}`;
`expires_at` — RFC3339, `source: "config"` для обнаруженных вне certbot.
Источники конфигов SSL: `/etc/nginx`, `/etc/apache2`, `/etc/httpd` (включая Debian sites-available/default).

Database SQL: `{name, size_bytes, owner: строка|null}` (owner у PostgreSQL).
Redis: `{name: "db0", size_bytes: null, keys, expires, avg_ttl}` — Redis
не сообщает размер отдельных логических БД; `avg_ttl` в миллисекундах CLI.
DatabaseQuery SQL: `{id, user, database, elapsed_sec, state, query}`.
Дампы создаются с `umask 077`, временный файл удаляется и при ошибке;
готовый файл публикуется атомарно без перезаписи. Произвольный SQL не принимается.
Возврат дампа — только путь на сервере, содержимое не передаётся клиенту.

Пример: `docker.stats` → `{"items":[{"id":"abc","name":"web",
"cpu_pct":0.5,"memory_used_bytes":13107200,"memory_limit_bytes":1073741824}]}`.

Не входят в этот набор: Docker Compose/pull/prune/PTY exec, редактирование и
включение/отключение сайтов, `redis.flush`, пользовательский SQL, прогресс дампа.

### 4.30 Диски — `disk.*`

Все методы чтения принимают `connection_id`. Команды выполняются в `LC_ALL=C`;
сканы используют `nice -n 19 ionice -c3`, `timeout` (TERM, затем KILL через 2 с).
Отсутствующая программа — `cap.missing {capability}`; невалидный вывод —
`remote.bad_output {command}`. GNU `du/find`, util-linux обязательны для сканов;
эти операции не эмулируются на несовместимых BusyBox-командах.

| Метод | Параметры помимо `connection_id` | Ответ |
|-------|---------------------------------|-------|
| `disk.list` | — | `{items: [DiskPartition], blockdevices: [...], filesystems: [...]}` |
| `disk.scan` | `path` (абсолютный), `depth` (2, 0–10), `one_filesystem` (false), `timeout_sec` (30, 1–120), `limit` (1000, 1–5000) | `{subscription}`; события ниже |
| `disk.scan.cancel` | `subscription` (без `connection_id`) | `{unsubscribed}`; чужая/неизвестная — `subscription.not_found` |
| `disk.large` | `path`, `min_size` (байты, 104857600), `one_filesystem`, `timeout_sec`, `limit`, `min_age_days`, `max_age_days`, `owner`, `extension` | DiskScanData |

DiskPartition: `{device, mount, total_bytes, used_bytes, available_bytes,
usage_pct, inodes_total, inodes_used, inodes_available, inodes_usage_pct}`.
Недоступные счётчики inode — `null`. `blockdevices` — дерево `lsblk --json
--bytes` (name, kname, type, size, fstype, mountpoint, children),
`filesystems` — дерево `findmnt --json` (target, source, fstype, options, children).

DiskScanData: `{items: [{path, size_bytes}], partial, truncated}`. Размеры du —
выделенное место (KiB переведены в байты), find — логическая длина файла.
Элементы отсортированы по убыванию размера, ограничены `limit`. Таймаут даёт
`partial: true`; незавершённая последняя запись отбрасывается. Прочие ошибки
команды не скрываются. Пути с пробелами, табуляцией и переводами строк
разбираются по NUL. Фильтры возраста соответствуют `find -mtime +N/-N` (полные 24-часовые периоды); `owner` — имя/UID, `extension` — расширение без шаблонов. Имена, не представимые в UTF-8, заменяются lossy-декодером.

События: `disk.scan.progress {subscription, connection_id, state: "started"}`,
затем `disk.scan.done {subscription, connection_id, data: DiskScanData}` либо
`error {subscription, error, fatal: true}`. Промежуточных процентов нет.
При отключении клиента подписка отменяется. Отмена прекращает ожидание и
события; удалённый скан в любом случае ограничен `timeout_sec`.

### 4.31 Журналы — `log.*`

| Метод | Параметры помимо `connection_id` | Ответ |
|-------|---------------------------------|-------|
| `log.sources` | `sudo` (false) | `{journal: bool, files: [абсолютный путь]}`; обычные файлы `/var/log` до глубины 2 |
| `log.query` | `unit`, `priority` (строка journalctl, например `"0..3"`), `since`, `until`, `grep` (regex), `boot` (строка, например `"0"`/`"-1"`), `lines` (100, 1–1000), `cursor`, `sudo` | LogQueryData |
| `log.follow` | те же фильтры, `interval_ms` (1000, 1000–60000) | `{subscription, interval_ms}`; события `log.line {subscription, data: LogEntry}` |
| `log.unfollow` | `subscription` (без `connection_id`) | `{unsubscribed}`; чужая/неизвестная — `subscription.not_found` |
| `log.file.tail` | `path` внутри `/var/log/`, `lines` (100, 1–5000), `sudo` | `{lines: [строка]}`; максимум 512 КиБ хвоста |
| `log.vacuum` | `confirm: true`, `max_bytes` (> 0) | `{vacuumed: true}` |

LogQueryData: `{items: [LogEntry], cursor: строка|null}`. LogEntry:
`{timestamp_usec: u64|null, cursor: строка|null, priority: u8|null,
unit: строка|null, message: строка|массив байтов|null, fields: объект journald}`.
Время — микросекунды Unix из `__REALTIME_TIMESTAMP`; все оригинальные поля
сохранены в `fields`, бинарные значения не преобразуются в строки.

`cursor` означает **после** записи (`--after-cursor`). Follow опрашивает
journald через отдельные exec и продвигает курсор только после успешного
чтения; данные доставляются через очередь клиента с backpressure. Первое
чтение возвращает последние `lines` записей. С курсором берутся первые `lines` записей после него (`-n +N`), без пропусков между пачками. При grep начальный обратный порядок journald нормализуется к хронологическому. Ошибка (в том числе устаревший
курсор) завершает подписку событием `error {subscription, error, fatal: true}`.
Пауза реализуется `unfollow`, возобновление — `follow` с последним курсором.
Отключение клиента отменяет подписки. Follow файлов в этой версии отсутствует.

Чтение по умолчанию без sudo; при нехватке прав фронт повторяет с `sudo:true`
после `priv.sudo.auth`. Vacuum всегда использует `ensure_writable` и
`exec_privileged`, без `confirm:true` — `validation.invalid {field:"confirm"}`.
Tail проверяет канонический путь (симлинки наружу `/var/log` запрещены).

### 4.32 Планировщик — `cron.*`

| Метод | Параметры помимо `connection_id` | Ответ |
|-------|---------------------------------|-------|
| `cron.list` | `user?` или `file?`, либо `system:true`; `sudo?` | CronTable; при `system:true` — `{files:[CronTable с path]}` |
| `cron.add` | `user` или `file`, `version`, `schedule`, `command`, `run_as` (для file), `confirm:true` | `{updated:true, version}` |
| `cron.set` | как add + `line` (номер строки с 1) | `{updated:true, version}` |
| `cron.del` | `user` или `file`, `version`, `line`, `confirm:true` | `{updated:true, version}` |
| `cron.toggle` | как del + `enabled` (bool) | `{updated:true, version}` |
| `cron.timer.list` | — | `{items:[Timer]}` |
| `cron.timer.enable` / `cron.timer.disable` | `unit` (имя `.timer`), `confirm:true` | `{updated:true}`; `systemctl enable/disable --now` |

`user` — имя пользователя (не shell-выражение); без источника `cron.list`
читает crontab текущего SSH-пользователя. `file` — имя непосредственно в
`/etc/cron.d`, только буквы/цифры/`_`/`-`. Симлинки не редактируются.
`system:true` читает обычные файлы `/etc/cron.d`, сохраняя их порядок строк.
Отсутствующий пользовательский crontab — пустая таблица; ошибки доступа
не превращаются в пустой результат. Все изменения и чтение чужого crontab/
системных файлов идут через `exec_privileged`; изменения — после
`ensure_writable`. Для записи `user` обязателен, если не указан `file`,
чтобы sudo не подменил целевого пользователя на root.

CronTable: `{version: sha256, items:[{line, raw, kind, enabled, schedule,
command, user}]}`. `kind`: `job|environment|comment|blank`. `schedule`,
`command`, `user` — `null` для неприменимых строк; `raw` сохраняет исходную
строку. Комментарии и окружение при изменении задачи сохраняются.
Отключение — маркер `#SHERPA_DISABLED `; обычные комментарии не считаются
отключёнными заданиями. `schedule` — пять полей cron (проверяются диапазоны,
списки, шаги, имена месяцев/дней) либо стандартный `@daily`/`@reboot` и т. п.
Команда — одна строка; `%` сохраняет обычную семантику cron.

`version` берётся из последнего `cron.list`: несовпадение — `cron.changed`.
Запись повторно сверяет SHA-256 под `flock` (ожидание до 10 с), затем
устанавливает crontab или атомарно заменяет файл cron.d через rename с правами
0644. Блокировка сериализует изменения Sherpa; внешние редакторы её не
соблюдают, поэтому абсолютная защита от внешней записи после сверки не
гарантируется. Stderr cron-команд не включается в ошибки: он может содержать
команду или окружение задания.

Timer: `{Id, ActiveState, UnitFileState, NextElapseUSecRealtime,
LastTriggerUSec, Unit}` — свойства `systemctl show` в виде строк; неизвестное
время — пустая строка. Перечисляются загруженные таймеры (`list-units --all`),
включая неактивные; создание unit-файлов этим API не выполняется.

---

### 4.12 SFTP — `files.*`

Все методы принимают `connection_id`, `path` — абсолютный POSIX-путь без
`..`. Канал SFTP (`russh-sftp`) открывается в общем SSH-соединении и входит
в общий лимит 8 каналов вместе с exec/direct-tcpip. `uid/gid` — числовые
владелец и группа (SFTP v3 не возвращает имена).

| Метод | Дополнительные параметры | Ответ |
|-------|--------------------------|-------|
| `files.list` | `limit` (1000, 1–5000) | `{items, truncated}` |
| `files.stat` | — | FileEntry |
| `files.mkdir` | — | `null` |
| `files.rename` | `to` (абсолютный путь) | `null` |
| `files.delete` | `recursive` (false); при true обязателен `confirm_token` | `null` |
| `files.chmod` | `mode` (число 0–4095), `privileged` (false) | `null` |
| `files.chown` | `uid`, `gid` (u32), `privileged` (false) | `null` |
| `files.read` | — | `{text, mtime, size}` |
| `files.write` | `text`, `mtime` из чтения | FileEntry |
| `files.search` | `query` (подстрока имени), `limit` (1000, ≤5000) | `{items, truncated}` |

FileEntry: `{path, name, type: file|directory|symlink|other, size, uid, gid,
permissions, mtime}`; отсутствующие атрибуты — null, mtime — Unix-секунды.
Редактор принимает только UTF-8 без NUL до 128 КиБ. Отличающийся mtime при
записи — `fs.conflict`; файл должен уже существовать и быть обычным файлом.
SFTP v3 не предоставляет атомарного compare-and-swap: внешнее изменение
между проверкой и записью, а также два изменения в одну секунду могут
не обнаружиться. Запись не является атомарной заменой.

Рекурсивное удаление не следует по симлинкам, корень `/` удалять нельзя.
Поиск не следует по симлинкам; обход ограничен 100000 записей. Для повышения
прав chmod/chown фронт явно передаёт `privileged: true`: команда выполняется
через общий `priv.*`, остальные файловые операции работают с правами SFTP.
Все изменения проверяют `ensure_writable()`.

### 4.13 Передачи — `transfer.*`

| Метод | Параметры | Ответ |
|-------|-----------|-------|
| `transfer.start` | `connection_id`, `direction: upload|download`, `local` (абсолютный локальный путь), `remote` (абсолютный POSIX-путь), `resume` (false) | `{id, status: queued}` |
| `transfer.list` | — | `{items: [{id, connection_id, data}]}` |
| `transfer.cancel` | `id` | `{cancelled: true}`; итог асинхронно |

Источник и назначение — точные пути файла/корневого каталога, а не каталог,
в который автоматически добавится basename. Очередь FIFO, две активные
передачи на демон, до 256 записей фоновых задач (завершённые вытесняются).
Файлы каталогов обходятся последовательно, без следования по симлинкам.
Существующий файл назначения без `resume: true` не перезаписывается.
Докачка сначала сверяет SHA-256 префикса, затем продолжает с его размера.
После каждого файла проверяются полный размер и SHA-256 источника и
назначения; цена проверки — дополнительное чтение файла по SFTP.
При отмене/ошибке частичный файл остаётся для следующего `start` с `resume`.
Смена readonly проверяется также между блоками загрузки.

Событие `transfer`: `{event:"transfer", id, connection_id, data}`;
`data.status`: `queued|running|completed|cancelled|failed`, `bytes` —
обработанные байты, при завершении `verified:"sha256"`, при ошибке `error`.
Промежуточные уведомления не чаще 200 мс; медленный клиент может пропустить
уведомление, актуальное состояние всегда доступно через `transfer.list`.
Отключение GUI не отменяет передачу; перезапуск демона теряет очередь,
частичные файлы сохраняются. Прогресс и итог принадлежат демону, доступны
всем локальным клиентам его защищённого сокета.

### 4.14 Туннели — `tunnel.*`

| Метод | Параметры | Ответ |
|-------|-----------|-------|
| `tunnel.start` | `connection_id`, `kind: local|remote|dynamic`, `bind_host` (127.0.0.1, IP), `bind_port`, для local/remote: `target_host`, `target_port` | `{id, bind_host, bind_port, status: starting}` |
| `tunnel.list` | — | `{items: [{id, connection_id, data}]}` |
| `tunnel.stop` | `id` | `{stopping: true}`; затем событие stopped |

local (-L) и dynamic (-D) используют `direct-tcpip`; remote (-R) —
`tcpip-forward`, входящие `forwarded-tcpip` и `cancel-tcpip-forward`.
`bind_port:0` выбирает свободный локальный порт, для remote требуется
явный ненулевой порт. dynamic — SOCKS5 CONNECT без авторизации, IPv4,
IPv6 и доменные имена (DNS на сервере); UDP/BIND не поддерживаются.
Нелокальный bind требует `danger.prepare`/`confirm_token`; учитывайте
GatewayPorts/AllowTcpForwarding на SSH-сервере. Старт требует writable.
На туннель допускается до 32 обслуживаемых клиентов, открытые SSH-каналы
дополнительно ограничены общим пулом; лишний входящий forwarded-tcpip
отклоняется до открытия. TCP использует backpressure.

Событие `tunnel`: `{event:"tunnel", id, connection_id, data}`; статусы
`starting|running|stopped|failed`, при ошибке `error`. Ошибка отдельного
TCP-клиента оставляет туннель running; ошибка соединения завершает его.
`stop` закрывает слушатель и обслуживаемые каналы. Туннель переживает
отключение GUI, удерживает демон от idle-exit и SSH от idle-reap.
При перезапуске демона туннели не восстанавливаются автоматически.
Уведомления восстанавливаются через list, как у передач.

### 4.15 Раскладка и подтверждения

`desktop.state.get {connection_id}` → `{connection_id, state}` (пустой
объект по умолчанию). `desktop.state.set {connection_id, state}` сохраняет
JSON-объект до 64 КиБ в SQLite ядра; SSH не требуется. Ядро не трактует
геометрию окон; последнее успешное сохранение побеждает.

`danger.prepare {method, params}` (также принимается `action` вместо
`method`) выдаёт `{confirm_token, expires_in_sec:60, action}`.
Фронт показывает параметры пользователю и добавляет токен к **тем же**
параметрам вызова указанного метода. Токен одноразовый, случайный 256-битный,
привязан к клиенту сокета, методу и параметрам, включая connection_id.
Порядок ключей JSON не влияет на сравнение. TTL — монотонные часы демона;
отключение клиента и перезапуск демона аннулируют токены. Не хранить токен
в настройках. Отдельного RPC `danger.confirm_token` нет: `confirm_token`
является параметром опасного метода, который атомарно потребляет токен.
API для других модулей описан в `UTILITY_GUIDE.md`.

### 4.20 Сеть — `net.*`

Все методы, кроме диагностики подписок, принимают `connection_id`.
Команды исполняются на сервере через пул SSH; firewall требует `priv.*`.

| Метод | Дополнительные параметры | Ответ |
|-------|-------------------------|-------|
| `net.iface.list` | — | `{items: [...]}` — объекты `ip -j address show`: `ifindex`, `ifname`, `flags`, `mtu`, `address`, `operstate`, `addr_info` (family/local/prefixlen/scope) |
| `net.route.list` | — | `{ipv4: [...], ipv6: [...]}` — все таблицы; объекты iproute2 (`dst`, `gateway?`, `dev?`, `metric?`, `table?`, `type?`) |
| `net.dns.get` | — | `{servers: [IP], search: [domain], options: [string], resolved: [{link, servers: [IP]}]}`; `/etc/resolv.conf`, дополнительно `resolvectl dns`, если доступен сервис |
| `net.sockets` | `privileged?` (false) | `{items: [{protocol, state, recv_queue, send_queue, local: {address, port}, peer: {address, port}, uid, processes: [{name,pid,fd}]}]}` — слушающие TCP/UDP, `ss -tulpnH -e` |
| `net.firewall.list` | `backend`: `ufw` / `nft` / `iptables` | `{backend, items, active?}` (см. ниже) |
| `net.apply` | параметры правила ниже; `confirm: true`, `confirm_token`, `confirm_timeout?` (60, 10–120 с) | `{transaction: UUID, confirm_timeout, state: "pending"}` |
| `net.confirm` | `transaction` | `{transaction, state: "confirmed"}` |
| `net.rollback` | `transaction` | `{transaction, state: "rolled_back"}` |

`port: null` означает wildcard. `uid: null` и пустой `processes` означают,
что ss не предоставил сведения (обычно нет прав на чужие процессы).
`privileged:true` собирает сокеты через root/sudo по правилам `priv.*`.
Неизвестные поля объектов iproute2/nftables сохраняются.
Правила UFW: `{id,destination,action,direction,source,comment}`;
`active` — bool. nftables: нативные JSON-объекты `nftables[]`, включая
таблицы, цепочки, правила с `handle` и выражения `expr`. iptables:
`{table, tokens: [string]}` — цепочки и правила `iptables-save` всех IPv4-таблиц.
Бэкенд выбирается явно из `Capabilities.tools`, чтобы не менять случайно
другой одновременно установленный firewall.

Параметры правила: `backend`, `operation: "add"|"delete"`,
`table?` (filter), `chain?` (INPUT для iptables, input для nft).
Добавление: `action: "allow"|"deny"|"reject"`, `protocol: "tcp"|"udp"`,
`port` (1–65535), `source?` (IPv4/CIDR, по умолчанию 0.0.0.0/0).
Удаление: `rule_id` — номер UFW/iptables либо handle nftables (> 0).
UFW добавляет входящее правило; nft работает с семейством `inet` и
существующими таблицей/цепочкой. Raw shell и произвольные nft-выражения
не принимаются. Перед `net.apply` фронт получает одноразовый
`confirm_token` через `danger.prepare {method: "net.apply", params: {...}}`
с теми же параметрами и показывает затрагиваемое правило пользователю.

`net.apply` проверяет readonly, подтверждение и наличие rollback-инструментов,
сохраняет снимок, запускает **серверный** watchdog, затем меняет правило.
Нужны `flock`, `nohup`, стандартные sh/coreutils; для iptables также
`iptables-save`/`iptables-restore`, для UFW — tar. На одном сервере возможна
только одна незавершённая сетевая транзакция. Lock сериализует применение,
подтверждение и откат; после истечения срока подтвердить откат нельзя.
Откат работает при потере SSH/клиента/демона и повторяется каждые 5 с при
ошибке восстановления. Ошибка применения также вызывает немедленный откат.
Снимок nft восстанавливается атомарной загрузкой ruleset с flush; iptables
сохраняет все IPv4-таблицы; UFW — конфигурацию и исходное active/inactive.
Снимки/состояние/лог watchdog остаются в root-only `/run/sherpa-net/<UUID>`
до перезагрузки. Сбой ОС/перезагрузка и внешние параллельные изменения
firewall не покрываются; фронт не должен предлагать внешние изменения во
время ожидания подтверждения. Сетевые адреса/маршруты/DNS пока только читаются.

### 4.21 Сетевые инструменты — `nettools.*`

| Метод | Параметры (помимо `connection_id`) | Ответ |
|-------|-----------------------------------|-------|
| `nettools.ping` | `target` (hostname/IP), `count?` (4, 1–30) | `{subscription}` |
| `nettools.trace` | `target` | `{subscription}` |
| `nettools.dig` | `target`, `record_type?`: A/AAAA/MX/TXT/NS (A) | `{subscription}` |
| `nettools.curl` | `target`: HTTP(S) URL без credentials | `{subscription}`; только HEAD, без redirect |
| `nettools.port` | `target`, `port` (1–65535) | `{subscription}`; TCP connect через nc |
| `nettools.cancel` | `subscription` | `{unsubscribed}`; только подписка текущего клиента |

Диагностика запускается **на сервере**, с `LC_ALL=C` и `timeout 60`.
Нужны соответствующая программа, timeout и nohup (Capabilities.tools).
Вывод stdout/stderr объединён и передаётся порциями не чаще раза в секунду:
`{"event":"nettools.output","subscription":"nt1","connection_id":"c1",
"method":"nettools.ping","records":[{"text":"64 bytes from ..."}]}`.
Строка `text` — диагностический вывод для консоли, фронт его не парсит.
Завершение: `{"event":"nettools.done","subscription":"nt1",
"exit_status":0,"truncated":false}`. Ненулевой exit_status — результат
диагностики (например, закрытый порт), 124 — удалённый timeout.
Ошибка транспорта/протокола — событие `error` с `fatal:true`.
Общий вывод ограничен 65536 байт; при достижении лимита поток завершается
с `truncated:true`, `exit_status` может быть null. Подписка удаляется после
завершения; отключение клиента/отмена прекращает доставку событий. Удалённая
команда при этом ограничена 60 с, временные файлы удаляются через 120 с
после завершения. Readonly не запрещает диагностику; создаются только
временные служебные файлы. speedtest не реализован.

---

### 4.40 Безопасность, бэкапы, команды, diff, заметки и аудит

Все удалённые методы принимают `connection_id`, кроме операций по ID задания
бэкапа: у них сервер берётся из задания. Чтение выполняется через RemoteExec;
`sec.*` использует `priv.*` для доступа к системным данным. Секреты передаются
только через JSON протокола. Следующие удаления, бан/разбан, добавление ключа,
prune, настоящий restore, reveal и каждый запуск произвольной команды требуют
одноразовый `confirm_token` из `danger.prepare` (§4.15), связанный с точными
параметрами и текущим клиентом. Изменения сервера проверяют `readonly`.

#### `sec.*`

| Метод | Параметры сверх `connection_id` | Ответ |
|-------|--------------------------------|-------|
| `sec.fail2ban.jails` | — | `{items: [имя jail]}` |
| `sec.fail2ban.bans` | `jail` | `{items: [IP]}` |
| `sec.fail2ban.ban`, `sec.fail2ban.unban` | `jail`, `ip`, `confirm_token` | `{success}` |
| `sec.logins` | — | `{items: [{message, ip}]}` |
| `sec.sessions` | — | `{items: [{user, terminal, login_at, source}]}` |
| `sec.keys.list` | `user?` (без него — все пользователи getent) | `{items: [{user?, line, key_type, public_key, comment, fingerprint, weak, bits}]}` |
| `sec.keys.add` | `user`, `public_key` (одна строка), `confirm_token` | `{success}` |
| `sec.keys.del` | `user`, `fingerprint`, `confirm_token` | `{deleted}` |
| `sec.ports` | — | `{items: [{protocol, state, address, port, process, risk}], scope, firewall_verified: false}` |
| `sec.updates` | — | `{items: [{name, details}]}` (APT security, текущий кэш) |
| `sec.score` | `user?` | `{score: 0..100, checks, recommendations}` |

`sec.logins`: journald за 24 часа, не более 10000 записей; fallback — последние
10000 строк `/var/log/auth.log` (содержимое файла может охватывать другой период).
`sec.ports` показывает слушатели вне loopback; доступность через firewall/NAT
не утверждается. Для закрытия порта применяется `net.*` с откатом.
DSA и RSA <2048 бит помечаются слабыми. При удалении ключа файл перечитывается,
проверяется SHA-256 под flock, результат пишется через временный файл и rename;
изменение исходного файла означает `sec.keys_changed` и требует обновить список.
Оценка: от 100 вычитаются 20 за отсутствие jails, до 30 за security-обновления
(3 за пакет), до 30 за слабые ключи (15 за ключ), до 30 за слушатели БД/Redis
(10 за порт). Это эвристика, не проверка соответствия стандартам безопасности.

#### `backup.*`

| Метод | Параметры | Ответ |
|-------|-----------|-------|
| `backup.jobs.list` | `connection_id?` | `{items: [BackupJob]}` |
| `backup.jobs.set` | `id?`, `connection_id`, `title`, `engine: restic\|rsync`, `source` (абсолютный), `repository`, `password_file?`, `privileged?`, `schedule?` | BackupJob |
| `backup.jobs.del` | `id`, `confirm_token` | `{deleted}` |
| `backup.run` | `id` | `{success, dry_run: false}` |
| `backup.snapshots` | `id` | `{items: [ResticSnapshot]}` |
| `backup.restore` | `id`, `snapshot`, `target` (абсолютный), `dry_run` (по умолчанию true), `confirm_token?` | `{success, dry_run}` |
| `backup.check`, `backup.prune` | `id`, для prune `confirm_token` | `{success, dry_run: false}` |

BackupJob — переданные поля плюс `id`, `updated_at`, после запуска
`last_run_at`, `last_run_ok`. `schedule` — сохранённое описание расписания;
данная группа не устанавливает cron и не запускает задания по таймеру.
Для restic обязателен путь `password_file` на сервере; пароль в БД задания
не хранится. Доступы S3/SFTP должны быть настроены на сервере. `privileged`
по умолчанию false; true выполняет команды через `priv.*`.
Для rsync поддерживается запуск (`-a -s`, без `--delete`), остальные операции
требуют restic. ResticSnapshot — объект `restic snapshots --json` с `id`,
`short_id`, `time`, `paths`, `hostname`, `tags?` и дополнительными полями restic.
Настоящий restore (`dry_run: false`) и prune требуют подтверждения.
Ошибки бэкапа не возвращают stderr, который может содержать данные хранилища.

#### `snippet.*`, `multi.run`

| Метод | Параметры | Ответ |
|-------|-----------|-------|
| `snippet.list`, `snippet.get` | для get `id` | `{items: [Snippet]}` / Snippet |
| `snippet.set` | `id?`, `title`, `command`, `tags?: [строка]` | Snippet `{id, title, command, tags}` |
| `snippet.del` | `id`, `confirm_token` | `{deleted}` |
| `snippet.run`, `multi.run` | `id` сниппета или `command`; `targets: [connection_id]`, либо `connection_id`, либо `group_id`, либо `all: true`; `confirm_token`, для >1 сервера `confirm_text: "выполнить"` | `{run, total}` |
| `snippet.history` | — | `{items: [{id, run, connection_id, timestamp, outcome}]}` |

Цели фиксируются перед запуском, дубликаты удаляются; 1–1000 целей, до 8
одновременно. По серверу приходит `multi.result` с `{run, connection_id,
ok, data: {exit_status, elapsed_ms, stdout, stderr}}` либо `error`; завершение
пачки — `multi.done {run, total, failed_tasks}`. Ненулевой exit_status —
результат команды; транспортная ошибка — `ok: false`. Отключение клиента
отменяет задачу и её ожидающие каналы; уже начатая удалённая команда может
продолжить выполнение. История хранит только метаданные, без текста команды
и вывода. Произвольный shell требует подтверждения даже на одном сервере.

#### `diff.*`

| Метод | Параметры | Ответ |
|-------|-----------|-------|
| `diff.compare` | `connection_id`, `path`; `other_connection_id`, `other_path?` либо `version_id` | `{equal, unified}` |
| `diff.save` | `connection_id`, `path` | `{id, connection_id, path, content}` |
| `diff.versions.list` | `connection_id?` | `{items: [сохранённая версия]}` |
| `diff.versions.del` | `id`, `confirm_token` | `{deleted}` |

Только абсолютные пути и UTF-8, не более 1 МиБ на файл. Unified diff содержит
один блок со всем контекстом, сохраняет отсутствие завершающего LF. Сохранённые
версии содержат текст файла в БД: для паролей используется хранилище секретов.

#### `notes.*`, `vault.*`

| Метод | Параметры | Ответ |
|-------|-----------|-------|
| `notes.list`, `notes.get` | `connection_id?` / `id` | `{items: [Note]}` / Note |
| `notes.set` | `id?`, `connection_id`, `title`, `markdown` | Note `{id, connection_id, title, markdown, updated_at}` |
| `notes.del` | `id`, `confirm_token` | `{deleted}` |
| `notes.secret.list`, `vault.list` | `connection_id?` | `{items: [SecretMetadata]}` |
| `notes.secret.set`, `vault.set` | `id?`, `connection_id`, `note_id?`, `title`, `value` | SecretMetadata |
| `notes.secret.del`, `vault.del` | `id`, `confirm_token` | `{deleted}` |
| `notes.secret.reveal`, `vault.reveal` | `id`, `confirm_token` | `{id, value, visible_for_sec: 10}` |

SecretMetadata — `{id, connection_id, note_id?, title, has_secret, updated_at}`;
значение отсутствует во всех ответах, кроме явного reveal. Шифрование — общий
ключ-файл и AEAD с AAD `vault:<id>:value`; замена ключа через force-new-key
удаляет недоступные секреты. Reveal записывается в аудит. Таймер сокрытия
и очистка буфера обмена — обязанность фронта.

#### `audit.*` и общий хук

`audit.list {connection_id?, client?, q?, limit?}` → `{items: [AuditEntry]}`.
`audit.export` с теми же фильтрами → `{csv}` (RFC4180, CRLF, защита формул).
`limit` по умолчанию 100, максимум 10000. AuditEntry — `{id, timestamp, client,
connection_id?, method, outcome}`. `client` — ID клиента демона, `q` ищет
в method/outcome. Записи старше 90 дней удаляются при добавлении записи.

Общий хук `daemon::registry::dispatch` записывает изменяющие вызовы всех
зарегистрированных групп (включая ошибки) по классификатору
`utilities::audit::is_mutating`; параметры, команды, stderr, секреты не пишутся.
Для новых нестандартных названий действий нужно расширить классификатор.
Другим модулям доступна функция `utilities::audit::record(store, client,
connection_id, method, outcome)`. Мультисерверные задачи дополнительно пишут
итог по каждому серверу. Локальные CLI-команды не проходят через этот хук.

### 4.50 Службы, пакеты и система — `svc.*`, `pkg.*`, `sys.*`

Во всех методах обязателен `connection_id`. Команды выполняются с `LC_ALL=C`,
изменения — через `ensure_writable` и `priv.*` (root/sudo). Нет инструмента —
`cap.missing`; ненулевой код — `remote.command_failed`; неожиданный формат —
`remote.bad_output`. Ответы содержат JSON, фронт не разбирает команды.

**Службы (systemd).**

| Метод | Дополнительные параметры | Ответ |
|-------|--------------------------|-------|
| `svc.list` | — | `{items: [Service]}`: установленные и загруженные service-юниты |
| `svc.status` | `unit` | Service; отсутствует — `svc.not_found` |
| `svc.logs` | `unit`, `lines` (100, максимум 2000) | `{items: [JournalEntry]}`; чтение через sudo для полного журнала |
| `svc.start`, `svc.stop`, `svc.restart`, `svc.reload`, `svc.enable`, `svc.disable` | `unit`, `confirm_token`, `confirm_hostname` | `{changed: true}` |

Service: `{name, description, load_state, active_state, sub_state, enabled,
user, pid, memory_bytes}`. `enabled` — строка systemd (`enabled`, `disabled`,
`static`, `masked`, …); неизвестная/неучитываемая память — `null`.
JournalEntry: `{timestamp_usec, priority, unit, identifier, message}`;
`timestamp_usec` — микросекунды Unix, `priority` — число 0–7 или `null`.
`message` сохраняет JSON-тип journald (обычно строка, для бинарного сообщения
возможен массив байтов). Все изменения служб требуют подтверждения с именем
сервера, включая sshd, сетевые и systemd-службы.

**Пакеты.** Менеджер выбирается из кэшированных Capabilities:
`apt`, `dnf`, `yum`, `pacman`, `apk`. Параметра для запуска произвольного
менеджера/команды нет. Для остальных менеджеров — `cap.missing`.

| Метод | Дополнительные параметры | Ответ |
|-------|--------------------------|-------|
| `pkg.detect` | — | `{manager}` |
| `pkg.installed` | — | PackageList |
| `pkg.search` | `query` (1–200 символов) | PackageList |
| `pkg.updates` | — | PackageList |
| `pkg.install`, `pkg.remove` | `packages` (1–200 имён), `confirm_token`; для remove также `confirm_hostname` | `{operation}` сразу; далее события |
| `pkg.upgrade` | `packages` (необяз.; отсутствие — все), `confirm_token`, `confirm_hostname` | `{operation}` сразу; далее события |

PackageList: `{manager, items: [{name, version, arch, description,
installed_version}]}`. Недоступные поля — `null`; поиск apt/dnf/yum не
сообщает версию, rpm/dnf/yum не сообщают прежнюю версию обновления. Читается
текущий локальный индекс: методы чтения не обновляют репозитории. Полный
upgrade pacman использует `-Syu`; обновление выбранных пакетов — `-S --needed`.
Установка/удаление/обновление не задают интерактивных вопросов. Ошибка
блокировки базы — `pkg.locked`.

События (только клиенту, вызвавшему операцию):

- `pkg.output {operation, connection_id, stream: "stdout"|"stderr", data_b64}` —
  блок вывода SSH в base64; границы UTF-8 и строк могут пересекать блоки;
- `pkg.done {operation, connection_id, data: {changed: true, exit_status: 0}}` —
  успешное завершение после последнего блока;
- `error {operation, connection_id, error, fatal: true}` — ошибка, вместо done.

Очереди ограничены, медленный клиент притормаживает чтение SSH. При отключении
клиента задача и канал отменяются; пакетная транзакция на сервере могла успеть
выполниться частично, поэтому после подключения следует перечитать состояние.

**Система.** В этой реализации время, пользователи и группы имеют префикс
`sys.*` (имена `time.*`/`user.*`/`group.*` в дизайн-макете — концептуальные).

| Метод | Дополнительные параметры | Ответ |
|-------|--------------------------|-------|
| `sys.info` | — | `{hostname, kernel, kernel_release, arch}` |
| `sys.hostname.set` | `hostname`, `confirm_token`, `confirm_hostname` | `{changed: true}`; через hostnamectl |
| `sys.time.get` | — | `{timezone, time, rtc_time, ntp, ntp_synchronized, can_ntp}`; timedatectl show |
| `sys.time.set_zone` | `timezone` (например `Europe/Moscow`) | `{changed: true}` |
| `sys.time.set` | `time` (дата/время timedatectl, например `2026-10-24 12:00:00`) | `{changed: true}`; NTP предварительно отключается отдельным явным вызовом |
| `sys.time.ntp` | `enabled` (bool, обяз.) | `{changed: true}` |
| `sys.user.list` | — | `{items: [SystemUser]}` |
| `sys.user.add` | `name`, `create_home` (false), `home?`, `shell?` (абсолютные пути) | `{changed: true}`; useradd |
| `sys.user.remove` | `name`, `remove_home` (false), `confirm_token`, `confirm_hostname` | `{changed: true}`; userdel, удаление root запрещено |
| `sys.user.passwd` | `name`, `password` (секрет) | `{changed: true}`; chpasswd, секрет только в stdin SSH, не в команде/ошибке |
| `sys.user.sudo` | `name`, `enabled` (bool, обяз.), `group?`: `sudo` или `wheel` | `{changed: true}`; без group выбирается существующая sudo, затем wheel |
| `sys.group.list` | — | `{items: [{name, gid, members: [name]}]}` |
| `sys.group.add` | `name` | `{changed: true}` |
| `sys.group.remove` | `name`, `confirm_token`, `confirm_hostname` | `{changed: true}`; удаление root запрещено |
| `sys.power` | `action`: `reboot` или `shutdown`, `delay_sec` (60, 60–86400), `confirm_token`, `confirm_hostname` | `{scheduled: true, delay_sec}`; задержка округляется вверх до минут shutdown |

SystemUser: `{name, uid, gid, description, home, shell, groups: [name]}`;
группы включают первичную и дополнительные. Парольные поля getent не выдаются.
Поля `time`, `rtc_time` — строки timedatectl в часовом поясе сервера, прочие
временные метки сохраняют общие соглашения. NTP-флаги — bool или `null`.
В системах без shadow-утилит (например, минимальный Alpine) чтение getent
работает, изменения без нужной программы возвращают `cap.missing`.

Capabilities.tools дополнен hostname/hostnamectl/timedatectl/getent,
useradd/userdel/usermod/groupadd/groupdel/gpasswd/chpasswd/shutdown,
dpkg-query/apt-cache/apt/rpm. Флаги utilities: `system` (hostname), `time`
(timedatectl), `users` (getent); `packages` включён для пяти реализованных
менеджеров.

**Подтверждение.** Перед опасным действием фронт получает hostname через
`sys.info`, просит пользователя ввести его и включает `confirm_hostname`
в `params` вызова `danger.prepare {method, params}` (§4.15). Затем вызывает
метод с **теми же** params плюс выданный `confirm_token`. Имя сверяется с
`hostname` сервера, билет одноразовый, привязан к клиенту/методу/всем параметрам
и действует 60 секунд. Установка пакетов требует билет, но не имя сервера.
Это также применяется к полному upgrade и смене hostname на любых серверах.

---

## 5. Каталог кодов ошибок

Коды групп §4.40: `document.not_found` (нет документа/секрета/задания),
`store.bad_document` (повреждённый JSON в БД), `backup.failed` (неуспешная
удалённая операция, `exit_status`), `diff.too_large` (>1 МиБ), `diff.binary`
(NUL в файле), `sec.key_not_found` (нет отпечатка), `sec.keys_changed`
(файл изменён после чтения). Общие `remote.bad_output`, `cap.missing`,
`remote.command_failed`, `priv.required`, `priv.readonly`,
`danger.confirmation_required`, `validation.*` применимы ко всем группам.


Формат: `<область>.<причина>`. Фронты держат таблицу переводов ru/en по
этим кодам; `params` — для подстановки.

### cron

| Код | Params | Описание |
|-----|--------|----------|
| `cron.changed` | — | Crontab изменился; перечитайте список и повторите редактирование |


### net — сетевые транзакции и диагностика

| Код | Params | Описание |
|-----|--------|----------|
| `net.pending` | `capability`, `exit_status` | На сервере уже есть незавершённая сетевая транзакция |
| `net.transaction_state` | `capability`, `exit_status` | Транзакция отсутствует, завершена или уже откатывается |
| `net.rollback` | `capability`, `exit_status` | Ошибка применения/восстановления; watchdog повторяет неуспешный откат, журнал на сервере |
| `nettools.timeout` | — | Диагностика не завершилась за 90 опросов |

Сетевые методы также используют общие `cap.missing`, `remote.bad_output`,
`remote.command_failed`, `priv.*`, `subscription.not_found`,
`danger.confirmation_required`, `validation.*`.

### request / protocol / internal

| Код | Params | Описание |
|-----|--------|----------|
| `request.unknown_command` | `command` | Неизвестная команда |
| `request.invalid_args` | `arg`, `reason` | Неверные или конфликтующие флаги |
| `request.invalid_json` | `offset` | stdin не является корректным JSON |
| `request.secret_in_args` | `arg` | Попытка передать секрет флагом |
| `request.too_many` | `max`, `given` | Превышен лимит (100 в пачке) |
| `protocol.unsupported` | `required`, `min`, `max` | Версия протокола фронта не поддерживается |
| `protocol.hello_required` | — | Демону не отправлен `hello` |
| `protocol.unknown_method` | `method` | Неизвестный метод демона |
| `internal` | `trace_id` | Внутренняя ошибка ядра (подробности в логе) |

### validation

| Код | Params | Описание |
|-----|--------|----------|
| `validation.required` | `field` | Обязательное поле не заполнено |
| `validation.invalid` | `field`, `reason` | Значение некорректно |
| `validation.too_long` | `field`, `max` | Слишком длинное значение |
| `validation.out_of_range` | `field`, `min`, `max` | Число вне диапазона (порт) |

### store / crypto / fs

| Код | Params | Описание |
|-----|--------|----------|
| `store.not_initialized` | `db` | Хранилище не создано, нужен `init` |
| `store.key_file_missing` | `path` | Нет ключ-файла, а в БД есть секреты |
| `store.permission_denied` | `path` | Нет прав на БД/ключ-файл/конфиг |
| `store.locked` | `path` | БД занята другим процессом дольше таймаута |
| `store.corrupted` | `path` | Файл БД повреждён |
| `store.schema_too_new` | `schema`, `supported` | БД создана более новым ядром |
| `store.migration_failed` | `from`, `to` | Ошибка миграции схемы |
| `crypto.key_mismatch` | `path` | Ключ-файл не подходит к БД |
| `crypto.key_file_invalid` | `path` | Ключ-файл повреждён или неверного размера |
| `crypto.key_file_insecure` | `path`, `mode` | Права ключ-файла шире 600 (предупреждение в `warnings`; на Windows не проверяется) |
| `crypto.decrypt_failed` | `field` | Не удалось расшифровать секрет |
| `config.invalid` | `path`, `line`, `reason` | Ошибка в конфиге |
| `fs.not_found` | `path` | Файл не найден (ключ, ssh config) |
| `fs.exists` | `path` | Файл назначения уже существует |
| `fs.read_failed` | `path`, `reason` | Не удалось прочитать файл |
| `fs.write_failed` | `path`, `reason` | Не удалось записать файл |

### conn / group / tag

| Код | Params | Описание |
|-----|--------|----------|
| `conn.not_found` | `id` | Подключение не найдено |
| `conn.name_taken` | `name`, `existing_id` | Имя подключения занято |
| `group.not_found` | `id` | Группа не найдена |
| `group.name_taken` | `name`, `parent_id` | Группа с таким именем уже есть на этом уровне |
| `group.cycle` | `id`, `parent_id` | Группа не может быть вложена в своего потомка |
| `tag.not_found` | `name` | Тег не найден |
| `tag.invalid` | `name` | Недопустимое имя тега |

### net (статус offline)

| Код | Params | Описание |
|-----|--------|----------|
| `net.dns_failed` | `host` | Имя хоста не разрешается |
| `net.refused` | `host`, `port` | Соединение отклонено (порт закрыт) |
| `net.unreachable` | `host` | Хост или сеть недоступны |
| `net.timeout` | `host`, `port`, `timeout_sec` | Таймаут подключения |
| `net.reset` | `host` | Соединение разорвано удалённой стороной |

### ssh (протокол)

| Код | Params | Описание |
|-----|--------|----------|
| `ssh.handshake_failed` | `reason` | Ошибка SSH-рукопожатия |
| `ssh.no_common_algorithm` | `kind`, `server` | Нет общих алгоритмов (kex/cipher/hostkey) |
| `ssh.not_ssh` | `banner` | На порту не SSH-сервер |
| `ssh.channel_failed` | `reason` | Сервер отказал в канале/PTY/шелле |
| `ssh.disconnected` | `reason` | Сервер закрыл соединение |

### auth (статус auth_failed)

| Код | Params | Описание |
|-----|--------|----------|
| `auth.rejected` | `user`, `methods` | Неверный пароль или ключ не принят |
| `auth.method_not_allowed` | `method`, `methods` | Сервер не разрешает этот способ входа |
| `auth.secret_required` | `kind` (`password`/`passphrase`) | Секрет не сохранён и не передан |
| `auth.key_invalid` | `reason` | Ключ не распознан / неподдерживаемый формат |
| `auth.passphrase_wrong` | — | Неверная парольная фраза ключа |
| `auth.key_file_not_found` | `path` | Файл ключа не найден |
| `auth.key_file_unreadable` | `path`, `reason` | Нет доступа к файлу ключа |
| `auth.cancelled` | — | Пользователь отменил ввод секрета |

### hostkey

| Код | Params | Описание |
|-----|--------|----------|
| `hostkey.unknown` | `host`, `port`, `key_type`, `fingerprint` | Ключ хоста ещё не известен (TOFU) |
| `hostkey.mismatch` | `host`, `port`, `key_type`, `fingerprint`, `known_fingerprint`, `known_accepted_at` | Ключ хоста изменился — возможна атака MITM |
| `hostkey.rejected` | `host`, `port`, `fingerprint` | Ключ ранее отклонён пользователем |
| `hostkey.fingerprint_changed` | `expected`, `actual` | При `accept` сервер предъявил другой ключ |
| `hostkey.not_found` | `host`, `port` | Сохранённого ключа нет |

### import

| Код | Params | Описание |
|-----|--------|----------|
| `import.parse_error` | `file`, `line`, `reason` | Синтаксическая ошибка в ssh config |
| `import.include_cycle` | `file` | Циклический `Include` (предупреждение) |
| `import.include_too_deep` | `file`, `max` | Слишком глубокая вложенность `Include` (предупреждение) |
| `import.match_unsupported` | `file`, `line` | Блок `Match` пропущен (предупреждение) |
| `import.multiple_identity_files` | `key`, `used` | Несколько `IdentityFile`, взят первый (предупреждение) |
| `import.source_changed` | `path` | Файл изменился после предпросмотра |
| `import.unknown_key` | `key` | В `select` ключ, которого нет в файле |

### daemon / session

| Код | Params | Описание |
|-----|--------|----------|
| `daemon.start_failed` | `reason` | Не удалось запустить демон |
| `daemon.not_running` | — | Демон не запущен |
| `daemon.already_running` | `pid` | Демон уже работает |
| `daemon.unreachable` | `endpoint` | Сокет есть, но демон не отвечает |
| `daemon.sessions_active` | `count` | Остановка отменена: есть активные сессии |
| `session.not_found` | `session` | Сессии нет (закрыта или чужая) |
| `session.not_connected` | `session`, `state` | Операция недоступна в текущем состоянии сессии |
| `session.prompt_expired` | `prompt_id` | Запрос уже неактуален |
| `session.limit_reached` | `max` | Превышено число сессий демона (по умолчанию 256) |
| `subscription.not_found` | `subscription` | Подписки нет (закрыта или чужая) |
| `auth.cancelled` | — | Пользователь отменил ввод секрета (также в §5 auth) |

### desktop / priv / remote

| Код | Params | Описание |
|-----|--------|----------|
| `priv.required` | `connection_id`, `sudo_available` | Нужны права root: введите пароль sudo |
| `priv.auth_failed` | `connection_id` | sudo отверг пароль |
| `priv.readonly` | `connection_id` | Соединение в режиме «только чтение» |
| `cap.missing` | `capability` | На сервере нет нужной возможности (`procfs`, `sudo`, …) |
| `remote.command_failed` | `command`, `exit_status`, `stderr` | Команда на сервере завершилась с ошибкой |
| `remote.bad_output` | `command` | Вывод команды на сервере не удалось разобрать |
| `svc.not_found` | `unit` | systemd-юнит не найден |
| `pkg.locked` | `manager` | База пакетного менеджера заблокирована другой операцией |
| `sys.password_failed` | `name` | Не удалось сменить пароль; диагностика скрыта для защиты секрета |
| `danger.hostname_mismatch` | — | Введённое имя не совпало с hostname сервера |
| `action.confirm_required` | — | Отсутствует явное `confirm: true` для Docker/Web/DB |
| `db.cancel_failed` | — | Сервер не отменил запрос или не отключил Redis-клиента |

---

### SFTP, передачи, туннели и подтверждения

| Код | Значение |
|-----|----------|
| `fs.channel` | Не удалось открыть подсистему SFTP |
| `fs.sftp`, `fs.timeout`, `fs.io` | Ошибка SFTP, таймаут или файлового ввода-вывода |
| `fs.not_found`, `fs.permission`, `fs.exists` | Нет файла, нет прав или назначение уже существует |
| `fs.not_file`, `fs.encoding`, `fs.too_large` | Не обычный файл, не UTF-8 текст или превышен лимит редактора |
| `fs.conflict` | mtime изменился; параметр `mtime` содержит текущее значение |
| `fs.bad_name`, `fs.limit` | Небезопасное имя записи или лимит обхода |
| `transfer.conflict` | Размер/префикс назначения несовместим с докачкой |
| `transfer.integrity` | Итоговый размер или SHA-256 не совпали |
| `transfer.symlink`, `transfer.cancelled` | Симлинк в пути передачи или отмена |
| `tunnel.failed`, `tunnel.exists` | Ошибка проброса или повтор remote listener |
| `job.limit`, `job.not_found` | Лимит фоновых задач или неизвестный id |
| `danger.limit`, `danger.confirmation_required` | Лимит токенов или нет действительного подтверждения |

## 6. Схема данных (контракт)

Поля, которые видит фронт. Внутреннее хранение (таблицы, шифрование) —
деталь реализации; секреты хранятся зашифрованными (AEAD, ключ из
ключ-файла) и наружу не отдаются.

### 6.1 Connection

| Поле | Тип | Примечание |
|------|-----|------------|
| `id` | uuid | |
| `name` | string | 1–128, уникально без учёта регистра |
| `host` | string | IP или DNS-имя |
| `port` | int | 1–65535, по умолчанию 22 |
| `user` | string \| null | |
| `auth` | Auth | см. ниже |
| `description` | string \| null | ≤ 4000 |
| `group_id` | uuid \| null | |
| `tags` | string[] | нормализованные |
| `status` | Status | |
| `source` | `"manual"` \| `"ssh_config"` \| `"duplicate"` | происхождение |
| `created_at`, `updated_at` | datetime | |

**Auth** (в ответах):

```json
{ "method": "password", "has_password": true }
{ "method": "key_file", "key_path": "/home/u/.ssh/id_ed25519", "has_passphrase": false }
{ "method": "key_text", "has_key_text": true, "key_type": "ssh-ed25519",
  "public_fingerprint": "SHA256:...", "has_passphrase": true }
```

Во входе — те же `method` плюс секретные поля `password`, `key_text`,
`passphrase` (§3.3).

### 6.2 Group

| Поле | Тип |
|------|-----|
| `id` | uuid |
| `name` | string, 1–128, уникально среди соседей |
| `parent_id` | uuid \| null |
| `connection_count` | int (только в `list`, без подгрупп) |
| `created_at`, `updated_at` | datetime |

### 6.3 Tag

| Поле | Тип |
|------|-----|
| `name` | string, 1–64, нижний регистр, без пробелов по краям; служит идентификатором |
| `connection_count` | int |

### 6.4 HostKey

| Поле | Тип | Примечание |
|------|-----|------------|
| `host` | string | как в подключении (без резолва) |
| `port` | int | |
| `key_type` | string | `ssh-ed25519`, `ecdsa-sha2-nistp256`, `rsa-sha2-512`… |
| `fingerprint` | string | `SHA256:<base64>` |
| `state` | `"trusted"` \| `"rejected"` | |
| `first_seen_at` | datetime | |
| `accepted_at` | datetime \| null | |
| `replaced_fingerprint` | string \| null | предыдущий ключ при замене |

### 6.5 Status

| Поле | Тип |
|------|-----|
| `state` | `online` \| `offline` \| `auth_failed` \| `error` \| `unknown` |
| `checked_at` | datetime \| null |
| `latency_ms` | int \| null |
| `error` | Error \| null (формат §1.4) |

Хранится только последний результат на подключение (история — после MVP).

### 6.6 Session (только в демоне, не в БД)

Другие объекты демона (Capabilities, PrivStatus, MonitorData) — §4.6–4.8.

| Поле | Тип |
|------|-----|
| `session` | string (`s1`, …), уникален в пределах демона |
| `connection_id` | uuid |
| `state` | `connecting` \| `connected` \| `closed` (вопросы — события, отдельного состояния нет) |
| `cols`, `rows` | int |
| `opened_at` | datetime |
| `attached` | bool |

### 6.7 Служебное

- `meta.schema_version` — версия схемы БД, миграции только вперёд;
  БД новее ядра → `store.schema_too_new`.
- `meta.key_fingerprint` — отпечаток ключа (проверка `crypto.key_mismatch`
  без попытки расшифровки секретов).
- `meta.key_last_export_at` — для `backup_recommended`.

---

## 7. Принятые решения

Все пункты подтверждены пользователем (2026-10-24). К п. 2 добавлено:
`conn check --stream` (§3.6).

1. Параметры через флаги **и** JSON в stdin (stdin перекрывает) — или только
   JSON для всех команд ради единообразия.
2. `conn check` по умолчанию отвечает одним объектом; `--stream` — JSON Lines
   (строка на сервер + итоговая строка).
3. Пачечная проверка не принимает новые host keys молча; `--accept-new` —
   явное согласие. Изменённый ключ — никогда автоматически.
4. Host key привязан к `(host, port)`, а не к подключению.
5. Уникальность имени подключения глобальная (а не в пределах группы) —
   нужна для правила дубликатов импорта.
6. Группы вложенные (дерево), удаление по умолчанию переносит содержимое
   в родителя.
7. Импорт: пустой `User` остаётся пустым (а не текущий пользователь ОС);
   `Match` не поддерживается.
8. Сессии демона живут 30 с после обрыва клиента (для переподключения
   фронта).
9. Права ключ-файла шире 600: в MVP — предупреждение (`warnings`), не отказ
   (на Windows права проверяются по ACL).
10. Удаление подключения не закрывает его открытые терминальные сессии.
