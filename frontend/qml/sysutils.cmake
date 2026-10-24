# Окна системных утилит: диски, логи, планировщик, службы, пакеты, система (общие компоненты — UtilPane, UTable, Fmt)
set_source_files_properties(qml/Fmt.qml PROPERTIES QT_QML_SINGLETON_TYPE TRUE)
list(APPEND SHERPA_QML
    qml/Fmt.qml
    qml/UtilPane.qml
    qml/UTable.qml
    qml/ServicesWindow.qml
    qml/SystemWindow.qml
    qml/DiskMap.qml
    qml/DisksWindow.qml
    qml/LogsWindow.qml
    qml/CronWindow.qml
    qml/PackagesWindow.qml
)
