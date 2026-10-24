cmake_minimum_required(VERSION 3.21)
project(SherpaFrontend VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

find_package(Qt6 6.5 REQUIRED COMPONENTS Core Gui Qml Quick QuickControls2 Test QuickTest LinguistTools)
qt_standard_project_setup(REQUIRES 6.5)

option(SHERPA_BUILD_TESTS "Build tests" ON)

set_source_files_properties(qml/Theme.qml qml/AppRegistry.qml PROPERTIES QT_QML_SINGLETON_TYPE TRUE)

set(SHERPA_QML
    qml/Theme.qml
    qml/AppRegistry.qml
    qml/Main.qml
    qml/HomePage.qml
    qml/Sidebar.qml
    qml/ServerList.qml
    qml/ServerCard.qml
    qml/ServerFormDialog.qml
    qml/ImportDialog.qml
    qml/HostKeyDialog.qml
    qml/FirstRunPage.qml
    qml/FatalPage.qml
    qml/SettingsDialog.qml
    qml/ConfirmDialog.qml
    qml/NameDialog.qml
    qml/SButton.qml
    qml/SChip.qml
    qml/SDialog.qml
    qml/STextField.qml
    qml/SSegmented.qml
    qml/SStatusDot.qml
    qml/SCheckBox.qml
    qml/SSwitch.qml
    qml/SSpinner.qml
    qml/SEmptyState.qml
    qml/SToast.qml
    qml/SBanner.qml
    qml/SSelect.qml
    qml/STagInput.qml
    qml/SLabel.qml
    qml/GlassPanel.qml
    qml/TabHost.qml
    qml/SWindow.qml
    qml/SMenu.qml
    qml/PromptDialog.qml
    qml/TerminalWindow.qml
    qml/MonitorWindow.qml
    qml/Taskbar.qml
    qml/Launcher.qml
    qml/ServersDock.qml
    qml/DesktopPage.qml
)

# Интерфейс — статическая QML-библиотека: её используют и приложение, и тесты.
qt_add_library(sherpa_ui STATIC)
qt_add_qml_module(sherpa_ui
    URI Sherpa
    VERSION 1.0
    QML_FILES ${SHERPA_QML}
    SOURCES
        src/CoreClient.cpp src/CoreClient.h
        src/ErrorText.cpp src/ErrorText.h
        src/ServerModel.cpp src/ServerModel.h
        src/Backend.cpp src/Backend.h
        src/DaemonClient.cpp src/DaemonClient.h
        src/TermEmulator.cpp src/TermEmulator.h
        src/TerminalSession.cpp src/TerminalSession.h
        src/DesktopManager.cpp src/DesktopManager.h
        src/TerminalItem.cpp src/TerminalItem.h
)
target_link_libraries(sherpa_ui PUBLIC Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick Qt6::QuickControls2)
target_include_directories(sherpa_ui PUBLIC src)

qt_add_resources(sherpa_ui "fonts"
    PREFIX "/fonts"
    BASE resources/fonts
    FILES
        resources/fonts/JetBrainsMono-Regular.ttf
        resources/fonts/JetBrainsMono-Bold.ttf
        resources/fonts/Inter-Regular.otf
        resources/fonts/Inter-SemiBold.otf
        resources/fonts/Inter-Bold.otf
)

qt_add_executable(sherpa src/main.cpp)
target_link_libraries(sherpa PRIVATE sherpa_ui sherpa_uiplugin Qt6::Gui Qt6::Quick Qt6::QuickControls2)
set_target_properties(sherpa PROPERTIES WIN32_EXECUTABLE ON)

# Переводы: ru и en (исходные тексты — английские)
qt_add_translations(sherpa_ui
    TS_FILES translations/sherpa_ru.ts translations/sherpa_en.ts
    RESOURCE_PREFIX /i18n
    SOURCES ${SHERPA_QML} src/Backend.cpp src/ErrorText.cpp
)

install(TARGETS sherpa RUNTIME DESTINATION bin)

if(SHERPA_BUILD_TESTS)
    enable_testing()
    add_subdirectory(tests)
endif()
