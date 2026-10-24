#!/usr/bin/env python3
"""Русские тексты окон системных утилит (диски, логи, планировщик, службы, пакеты, система).
Дополняет scripts/ru_translations.py: заполняет только непереведённые записи translations/sherpa_ru.ts.
Запуск (после lupdate): python3 scripts/ru_sysutils.py [translations/sherpa_ru.ts]"""
import sys
RU = {
"Administrator rights": "Права администратора",
"This action needs sudo. The password is kept only in the daemon memory until the connection closes.": "Для этого действия нужен sudo. Пароль хранится только в памяти демона до закрытия соединения.",
"sudo password": "Пароль sudo", "Authenticate": "Подтвердить", "Confirm action": "Подтверждение действия",
"To confirm, type the server name: %1": "Для подтверждения введите имя сервера: %1", "Confirm": "Подтвердить",
"B": "Б", "KB": "КБ", "MB": "МБ", "GB": "ГБ", "TB": "ТБ", "PB": "ПБ",
"emerg": "аварийно", "alert": "тревога", "crit": "критично", "err": "ошибка", "warning": "предупреждение", "notice": "заметка", "info": "инфо", "debug": "отладка",
"Start": "Запустить", "Stop": "Остановить", "Restart": "Перезапустить", "Reload": "Перечитать конфиг", "Enable autostart": "Включить автозапуск", "Disable autostart": "Выключить автозапуск",
"The action is applied on the server immediately. Stopping or disabling network, SSH or systemd services may cut off access.": "Действие применяется на сервере сразу. Остановка или отключение сетевых служб, SSH и systemd может лишить вас доступа.",
"Waiting for the server…": "Ожидание сервера…", "systemd was not found on this server.": "На этом сервере не найден systemd.",
"All": "Все", "Running": "Запущены", "Failed": "Сбойные", "Autostart": "Автозапуск", "Search services…": "Поиск служб…", "Search services": "Поиск служб",
"Refresh": "Обновить", "No services match the filter": "Нет служб по фильтру", "State": "Состояние", "Unit": "Юнит", "Memory": "Память", "Active": "Активна",
"Journal": "Журнал", "No journal entries": "Нет записей журнала", "h ": "ч ", "m": "м",
"General": "Общее", "Time": "Время", "Users": "Пользователи", "Operating system": "Операционная система", "Kernel": "Ядро", "Architecture": "Архитектура",
"Uptime": "Время работы", "Current user": "Текущий пользователь", "Host name": "Имя хоста", "Change": "Изменить", "Change host name": "Изменить имя хоста",
"The host name will be changed to the one below. Services and monitoring that rely on the old name may break.": "Имя хоста будет изменено на указанное ниже. Службы и мониторинг, привязанные к старому имени, могут перестать работать.",
"Host name changed": "Имя хоста изменено", "Power": "Питание",
"The command is scheduled with a delay (from 1 minute); the terminal and utilities lose the connection when the server goes down.": "Команда выполняется с задержкой (от 1 минуты); когда сервер выключится, терминал и утилиты потеряют соединение.",
"Delay, min": "Задержка, мин", "Reboot…": "Перезагрузить…", "Shut down…": "Выключить…",
"Time settings are not available on this server: %1": "Настройки времени недоступны на этом сервере: %1",
"Server time": "Время сервера", "RTC": "RTC", "Time zone": "Часовой пояс", "Synchronized": "Синхронизировано", "yes": "да", "no": "нет",
"Automatic time sync (NTP)": "Автоматическая синхронизация времени (NTP)", "Apply": "Применить", "Time zone changed": "Часовой пояс изменён",
"Set time manually": "Установить время вручную", "Set": "Установить", "Time set": "Время установлено", "Turn off NTP first to set the time manually.": "Чтобы задать время вручную, сначала выключите NTP.",
"Search users…": "Поиск пользователей…", "Search users": "Поиск пользователей", "Add user": "Добавить пользователя", "Login": "Логин", "Home": "Домашний каталог", "Shell": "Оболочка",
"Administrator (sudo)": "Администратор (sudo)", "Change password…": "Сменить пароль…", "Delete user…": "Удалить пользователя…", "Add group": "Добавить группу",
"Members": "Участники", "Delete group…": "Удалить группу…", "Delete group %1": "Удалить группу %1", "The group will be removed from the server.": "Группа будет удалена с сервера.",
"Group deleted": "Группа удалена", "Reboot the server": "Перезагрузить сервер", "Shut down the server": "Выключить сервер",
"The server will reboot in %1 min. All sessions will be interrupted.": "Сервер перезагрузится через %1 мин. Все сессии прервутся.",
"The server will shut down in %1 min. You will need physical or provider access to start it again.": "Сервер выключится через %1 мин. Чтобы включить его снова, понадобится физический доступ или доступ через провайдера.",
"Reboot": "Перезагрузить", "Shut down": "Выключить", "Scheduled: %1": "Запланировано: %1", "reboot": "перезагрузка", "shutdown": "выключение",
"Enter a login": "Введите логин", "User created": "Пользователь создан", "Password (optional)": "Пароль (необязательно)", "Create home directory": "Создать домашний каталог",
"Create": "Создать", "Change password": "Смена пароля", "Password changed": "Пароль изменён", "New password for %1": "Новый пароль для %1", "Delete user": "Удаление пользователя",
"User %1 will be deleted from the server.": "Пользователь %1 будет удалён с сервера.", "Also delete the home directory": "Удалить и домашний каталог", "Continue…": "Продолжить…",
"Delete user %1": "Удалить пользователя %1", "The user and the home directory will be deleted. This cannot be undone.": "Пользователь и его домашний каталог будут удалены. Это нельзя отменить.",
"The user will be deleted; the home directory is kept.": "Пользователь будет удалён, домашний каталог сохранится.", "User deleted": "Пользователь удалён",
"Install": "Установить", "The packages below will be installed with their dependencies.": "Перечисленные пакеты будут установлены вместе с зависимостями.",
"Remove": "Удалить", "The packages below will be removed from the server together with unused dependencies, if the manager decides so.": "Перечисленные пакеты будут удалены с сервера, вместе с ненужными зависимостями, если так решит менеджер пакетов.",
"Upgrade selected": "Обновить выбранные", "Upgrade all": "Обновить все", "The selected packages will be upgraded to the latest versions.": "Выбранные пакеты будут обновлены до последних версий.",
"All installed packages will be upgraded. Services may restart.": "Будут обновлены все установленные пакеты. Службы могут перезапуститься.", "Done": "Готово", "Error": "Ошибка",
"No supported package manager (apt, dnf, yum, pacman, apk) was found.": "Не найден поддерживаемый менеджер пакетов (apt, dnf, yum, pacman, apk).",
"Updates": "Обновления", "Installed": "Установленные", "Package name to search, then Enter": "Имя пакета для поиска, затем Enter", "Filter installed packages…": "Фильтр установленных пакетов…",
"Package search": "Поиск пакетов", "Upgrade all…": "Обновить все…", "Install selected": "Установить выбранные", "Remove selected…": "Удалить выбранные…",
"%1 updates available": "Доступно обновлений: %1", "%1 packages": "Пакетов: %1", "%1 found": "Найдено: %1", "System is up to date": "Система обновлена",
"Package": "Пакет", "New": "Новая", "Now": "Сейчас", "Arch": "Архитектура", "Version": "Версия", "Description": "Описание", "Type a name and press Enter": "Введите имя и нажмите Enter",
"Nothing found": "Ничего не найдено", "Operation log": "Журнал операции",
"every year": "каждый год", "every month": "каждый месяц", "every week": "каждую неделю", "every day at 00:00": "каждый день в 00:00", "every hour": "каждый час", "at boot": "при загрузке",
"every minute": "каждую минуту", "every %1 min": "каждые %1 мин", "every hour at :%1": "каждый час в :%1", "every %1 h at :%2": "каждые %1 ч в :%2", "every day at %1": "каждый день в %1",
"every %1 at %2": "по %1 в %2", "Sun": "воскресеньям", "Mon": "понедельникам", "Tue": "вторникам", "Wed": "средам", "Thu": "четвергам", "Fri": "пятницам", "Sat": "субботам",
"on day %1 of the month at %2": "%1-го числа в %2", "weekdays at %1": "по будням в %1", "Job disabled": "Задача отключена", "Job enabled": "Задача включена",
"cron was not found on this server.": "На этом сервере не найден cron.", "User cron": "Cron пользователя", "System cron": "Системный cron", "systemd timers": "Таймеры systemd",
"Crontab user": "Пользователь crontab", "New job": "Новая задача", "No jobs": "Нет задач", "active": "активна", "disabled": "отключена", "Schedule": "Расписание", "Meaning": "Смысл",
"Command": "Команда", "File": "Файл", "Next runs": "Ближайшие запуски", "Edit…": "Изменить…", "Disable": "Отключить", "Enable": "Включить", "Delete…": "Удалить…",
"No timers": "Нет таймеров", "Timer": "Таймер", "Next run": "След. запуск", "Last run": "Посл. запуск", "enabled": "включён", "Delete job?": "Удалить задачу?",
"The job will be removed from the crontab:\n%1  %2": "Задача будет удалена из crontab:\n%1  %2", "Job deleted": "Задача удалена", "Edit job": "Изменение задачи",
"Enter a command": "Введите команду", "Invalid schedule": "Неверное расписание", "File name: letters, digits, _ and - only": "Имя файла: только буквы, цифры, _ и -",
"Job saved": "Задача сохранена", "Minute": "Минута", "Hour": "Час", "Day": "День", "Month": "Месяц", "Weekday": "День недели", "every 5 min": "каждые 5 мин", "hourly": "ежечасно",
"daily 03:00": "ежедневно 03:00", "weekly": "еженедельно", "monthly": "ежемесячно", "Cron expression": "Выражение cron", "Next runs:": "Ближайшие запуски:",
"The job runs every minute — make sure this is intended.": "Задача запускается каждую минуту — убедитесь, что так и задумано.", "File in /etc/cron.d": "Файл в /etc/cron.d", "Run as": "Запускать от",
"15 min": "15 мин", "1 h": "1 ч", "Today": "Сегодня", "24 h": "24 ч", "Any level": "Любой уровень", "Errors": "Ошибки", "Warnings and worse": "Предупреждения и хуже",
"Notice and worse": "Заметки и хуже", "Info and worse": "Инфо и хуже", "All boots": "Все загрузки", "This boot": "Эта загрузка", "Previous boot": "Прошлая загрузка",
"Follow stopped": "Слежение остановлено", "journalctl was not found on this server.": "На этом сервере не найден journalctl.", "Sources": "Источники", "System journal": "Системный журнал",
"Unit (nginx.service)": "Юнит (nginx.service)", "Unit filter": "Фильтр по юниту", "Search / regex…": "Поиск / regex…", "Search in logs": "Поиск в журналах", "sudo": "sudo",
"⏸ Pause": "⏸ Пауза", "▶ Follow": "▶ Следить", "Copy": "Копировать", "Save…": "Сохранить…", "No entries match the filter": "Нет записей по фильтру", "%1 entries": "Записей: %1",
"following": "слежение", "Vacuum journal…": "Очистить журнал…", "Could not write the file": "Не удалось записать файл", "Vacuum journal": "Очистка журнала",
"Old journal files will be deleted until the journal is smaller than the limit. This cannot be undone.": "Старые файлы журнала будут удалены, пока журнал не станет меньше лимита. Это нельзя отменить.",
"Keep at most, MB": "Оставить не больше, МБ", "Vacuum": "Очистить", "Journal vacuumed": "Журнал очищен", "(files)": "(файлы)", "Scan failed": "Сканирование не удалось",
"The required tools (df, lsblk, du, find) were not found.": "Не найдены нужные утилиты (df, lsblk, du, find).", "Partitions": "Разделы", "Map": "Карта", "Large files": "Крупные файлы",
"No partitions": "Нет разделов", "Device": "Устройство", "Mount": "Точка монтирования", "Size": "Размер", "Used": "Занято", "Usage": "Заполнение",
"Double-click a partition to analyze its contents.": "Двойной щелчок по разделу — анализ его содержимого.", "Analyze selected": "Анализировать выбранный", "Path to analyze": "Путь для анализа",
"depth 1": "глубина 1", "depth 2": "глубина 2", "depth 3": "глубина 3", "one filesystem": "одна ФС", "Scan": "Сканировать", "List": "Список", "partial": "частично", "truncated": "усечено",
"Share": "Доля", "Largest": "Крупнейшие", "Scanning %1…": "Сканирование %1…", "Pick a path and press Scan": "Укажите путь и нажмите «Сканировать»",
"Low priority (nice/ionice), up to 60 s": "Низкий приоритет (nice/ionice), до 60 с", "Path": "Путь", "Min size, MB": "Мин. размер, МБ", "Extension": "Расширение",
"Older, days": "Старше, дней", "Find": "Найти", "Searching…": "Поиск…", "Press Find to search for large files": "Нажмите «Найти», чтобы искать крупные файлы",
"partial: timeout": "частично: таймаут", "%1 files · double-click copies the path": "Файлов: %1 · двойной щелчок копирует путь",
"Services": "Службы", "Disks": "Диски", "Logs": "Журналы", "Packages": "Пакеты", "System": "Система", "Scheduler": "Планировщик",
"Name": "Имя", "Group": "Группа", "Members": "Участники", "Add group": "Добавить группу", "Group name": "Имя группы",
"Show with sudo": "Показать через sudo", "The journal of this service needs administrator rights": "Журнал этой службы доступен только администратору",
"Cancel": "Отмена", "Not available on this server": "Недоступно на этом сервере", "Memory": "Память", "Search": "Поиск",
"Add": "Добавить", "Change…": "Изменить…", "Delete": "Удалить", "Save": "Сохранить",
"Command": "Команда", "Dismiss": "Закрыть",
}
import xml.etree.ElementTree as ET
p = sys.argv[1] if len(sys.argv) > 1 else "translations/sherpa_ru.ts"
tree = ET.parse(p)
missing = set()
for m in tree.getroot().iter("message"):
    loc = m.find("location")
    ctx_file = loc.get("filename", "") if loc is not None else ""
    tr = m.find("translation")
    if tr.get("type") != "unfinished" and (tr.text or "").strip():
        continue
    src = m.find("source").text or ""
    if src in RU:
        tr.text = RU[src]
        tr.attrib.pop("type", None)
    elif any(n in ctx_file for n in ("Window.qml", "UtilPane", "UTable", "DiskMap", "SysApps", "Fmt.qml")):
        missing.add(src)
with open(p, "w", encoding="utf-8") as f:
    f.write('<?xml version="1.0" encoding="utf-8"?>\n<!DOCTYPE TS>\n')
    f.write(ET.tostring(tree.getroot(), encoding="unicode"))
    f.write("\n")
if missing:
    print("НЕ ПЕРЕВЕДЕНО:", *sorted(missing), sep="\n  ")
