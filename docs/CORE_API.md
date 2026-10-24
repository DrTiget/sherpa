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

## 4. Демон терминала

### 4.1 Запуск и обнаружение

- Демон — `sherpa-core daemon` (тот же бинарник). Фронт не запускает его
  вручную, а вызывает:

```
sherpa-core daemon ensure
```

  Команда проверяет, жив ли демон (рукопожатие по сокету), при
  необходимости запускает его отсоединённым процессом и ждёт готовности
  (≤ 5 с). Ответ:

```json
{ "ok": true, "protocol": 1, "data": {
  "endpoint": "/run/user/1000/sherpa/daemon.sock", "pid": 4242,
  "started": true, "core_version": "0.1.0", "protocol": 1 } }
```

- Адрес сокета:
  - Linux: `$XDG_RUNTIME_DIR/sherpa/daemon.sock` (каталог 700, сокет 600);
    без `XDG_RUNTIME_DIR` — `~/.local/state/sherpa/daemon.sock`.
  - Windows: named pipe `\\.\pipe\sherpa-core-<SID пользователя>` с DACL
    только для текущего пользователя.
  - Если задан нестандартный `--db`, к имени добавляется короткий хеш пути
    БД, чтобы разные хранилища не делили один демон.
- Один демон на пользователя/хранилище: эксклюзивный lock-файл рядом с
  сокетом; второй экземпляр завершается с `daemon.already_running`.
- Устаревший сокет (процесса нет) удаляется при старте.
- `sherpa-core daemon status` — `{running, pid, endpoint, sessions, uptime_sec}`.
  `sherpa-core daemon stop [--force]` — без `--force` отказ
  (`daemon.sessions_active`), если есть сессии.
- Авто-завершение: нет ни одной сессии и ни одного клиента дольше
  `daemon_idle_timeout_sec` (по умолчанию 300 с).
- Если версия демона отличается от версии бинарника фронта (`hello`
  вернул другой `core_version`), а сессий нет — фронт вызывает
  `daemon stop` и `ensure` заново; с сессиями — работает со старым, если
  протокол совместим.

### 4.2 Протокол поверх сокета

JSON Lines: один JSON-объект на строку, `\n`-разделитель, UTF-8, строка
≤ 1 МиБ. Сообщения трёх видов:

| Вид | Направление | Форма |
|-----|-------------|-------|
| Запрос | клиент → демон | `{"id": 7, "method": "...", "params": {...}}` |
| Ответ | демон → клиент | `{"id": 7, "ok": true, "data": {...}}` или `{"id": 7, "ok": false, "error": {...}}` |
| Событие | демон → клиент | `{"event": "...", "session": "s1", ...}` (без `id`) |

- `id` — число, уникальное в пределах соединения клиента.
- `error` — тот же формат, что в §1.4.
- Бинарные данные терминала (ввод и вывод) — **base64** (стандартный, с
  паддингом) в поле `data_b64`.
- Один клиент может держать много сессий. Сессия принадлежит клиенту,
  который её открыл; при обрыве соединения клиента сессии **не**
  закрываются сразу, а ждут переподключения `session.attach` 30 с
  (решение на подтверждение), затем закрываются.

### 4.3 Методы

**`hello`** — первым сообщением, иначе `protocol.hello_required`.

```json
{"id":1,"method":"hello","params":{"protocol":1,"client":"sherpa-gtk/0.1.0"}}
{"id":1,"ok":true,"data":{"protocol":1,"core_version":"0.1.0","daemon_pid":4242,"sessions":["s1"]}}
```

**`session.open`** — открыть PTY-сессию к подключению.

```json
{"id":2,"method":"session.open","params":{
  "connection_id":"c1","cols":120,"rows":34,"term":"xterm-256color",
  "env":{"LANG":"ru_RU.UTF-8"},
  "secrets":{"password":"..."}   // необязательно, разовые секреты
}}
{"id":2,"ok":true,"data":{"session":"s1"}}
```

Ответ приходит сразу; ход подключения — событиями. Секреты в `params` —
допустимы: сокет доступен только владельцу.

**`session.input`**

```json
{"id":3,"method":"session.input","params":{"session":"s1","data_b64":"bHMgLWxhCg=="}}
```

Ответ `{"id":3,"ok":true,"data":null}` (клиент может не ждать каждого).

**`session.resize`**

```json
{"id":4,"method":"session.resize","params":{"session":"s1","cols":160,"rows":40}}
```

**`session.hostkey_answer`** — ответ на событие `hostkey_prompt`.

```json
{"id":5,"method":"session.hostkey_answer","params":{"session":"s1","prompt_id":"p1","decision":"accept"}}
```

`decision`: `accept` (сохранить и продолжить), `accept_once` (продолжить
без сохранения), `reject` (запомнить отклонение и закрыть), `cancel`
(закрыть без запоминания). Для изменённого ключа `accept` означает
замену сохранённого.

**`session.auth_answer`** — ответ на `auth_prompt` (пароль/фраза не сохранены
или отвергнуты; keyboard-interactive).

```json
{"id":6,"method":"session.auth_answer","params":{"session":"s1","prompt_id":"p2",
  "responses":["s3cr3t"],"save":false}}
```

`save: true` — сохранить введённый пароль/фразу в подключение.

**`session.close`**

```json
{"id":7,"method":"session.close","params":{"session":"s1"}}
```

**`session.list`** — `{"items":[{"session":"s1","connection_id":"c1","state":"connected","opened_at":"...","cols":120,"rows":34,"attached":true}]}`.

**`session.attach`** — подхватить сессию после переподключения клиента
(`{"session":"s1"}`); демон досылает хвост вывода из буфера (до 64 КиБ).

**`ping`** — `{"data":{"pong":true}}`.

### 4.4 События

| `event` | Поля | Когда |
|---------|------|-------|
| `connecting` | `session` | Начато TCP-подключение |
| `hostkey_prompt` | `session, prompt_id, kind: "unknown"\|"mismatch", host, port, key_type, fingerprint, known_fingerprint?` | Нужно решение TOFU |
| `auth_prompt` | `session, prompt_id, kind: "password"\|"passphrase"\|"keyboard_interactive", prompts: [{text, echo}], attempt` | Нужен секрет |
| `connected` | `session, server_banner` | PTY открыт, шелл запущен |
| `output` | `session, data_b64` | Данные из PTY (stdout+stderr) |
| `closed` | `session, reason: "exit"\|"client"\|"remote"\|"error"\|"idle", exit_status?, signal?` | Сессия завершена; последнее событие сессии |
| `error` | `session, error: {code, message, params}`, `fatal: bool` | Ошибка; при `fatal` следом придёт `closed` |
| `daemon_shutdown` | `reason: "idle"\|"stop"` | Демон завершается |

Пример потока:

```
→ {"id":2,"method":"session.open","params":{"connection_id":"c1","cols":120,"rows":34}}
← {"id":2,"ok":true,"data":{"session":"s1"}}
← {"event":"connecting","session":"s1"}
← {"event":"hostkey_prompt","session":"s1","prompt_id":"p1","kind":"unknown","host":"10.0.0.5","port":22,"key_type":"ssh-ed25519","fingerprint":"SHA256:abc..."}
→ {"id":3,"method":"session.hostkey_answer","params":{"session":"s1","prompt_id":"p1","decision":"accept"}}
← {"id":3,"ok":true,"data":null}
← {"event":"connected","session":"s1","server_banner":"SSH-2.0-OpenSSH_9.6p1"}
← {"event":"output","session":"s1","data_b64":"V2VsY29tZS4uLg0K"}
→ {"id":4,"method":"session.input","params":{"session":"s1","data_b64":"ZXhpdA0="}}
← {"event":"closed","session":"s1","reason":"exit","exit_status":0}
```

Поток вывода: демон склеивает вывод PTY в события раз в ≤ 10 мс или по
32 КиБ. Если клиент не читает и буфер сессии превышает 1 МиБ, демон
перестаёт читать из канала SSH (backpressure), данные не теряются.

---

## 5. Каталог кодов ошибок

Формат: `<область>.<причина>`. Фронты держат таблицу переводов ru/en по
этим кодам; `params` — для подстановки.

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

---

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

| Поле | Тип |
|------|-----|
| `session` | string (`s1`, …), уникален в пределах демона |
| `connection_id` | uuid |
| `state` | `connecting` \| `awaiting_hostkey` \| `awaiting_auth` \| `connected` \| `closed` |
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
