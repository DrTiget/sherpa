# Окна утилит: Сеть, Сетевые инструменты, Docker, Веб, БД, Безопасность, Бэкапы, Сниппеты, Мультисервер,
# Сравнение конфигов, Заметки и секреты, Журнал действий, виджеты на обоях.
list(APPEND SHERPA_QML
    qml/tools/ToolRpc.qml
    qml/tools/ToolPane.qml
    qml/tools/ToolDangerDialog.qml
    qml/tools/DataTable.qml
    qml/tools/STextArea.qml
    qml/tools/ServerPicker.qml
    qml/tools/MultiRunner.qml
    qml/tools/RunResults.qml
    qml/tools/NetApplyFlow.qml
    qml/tools/NetworkWindow.qml
    qml/tools/NettoolsWindow.qml
    qml/tools/DockerWindow.qml
    qml/tools/WebWindow.qml
    qml/tools/DatabasesWindow.qml
    qml/tools/SecurityWindow.qml
    qml/tools/BackupWindow.qml
    qml/tools/SnippetsWindow.qml
    qml/tools/MultiWindow.qml
    qml/tools/DiffWindow.qml
    qml/tools/NotesWindow.qml
    qml/tools/AuditWindow.qml
    qml/tools/DesktopWidgets.qml
    qml/tools/FirewallWindow.qml
)
set(SHERPA_TOOLS_SOURCES src/ToolUtil.cpp src/ToolUtil.h)
