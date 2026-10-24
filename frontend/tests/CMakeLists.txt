add_executable(tst_coreclient tst_coreclient.cpp)
target_link_libraries(tst_coreclient PRIVATE sherpa_ui Qt6::Test)
add_test(NAME coreclient COMMAND tst_coreclient)

add_executable(tst_screens tst_screens.cpp)
target_link_libraries(tst_screens PRIVATE sherpa_ui sherpa_uiplugin Qt6::Test Qt6::Quick Qt6::QuickControls2)
add_test(NAME screens COMMAND tst_screens)
set_tests_properties(screens PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")

add_executable(tst_terminal tst_terminal.cpp)
target_link_libraries(tst_terminal PRIVATE sherpa_ui Qt6::Test)
add_test(NAME terminal COMMAND tst_terminal)

add_executable(tst_live tst_live.cpp)
target_link_libraries(tst_live PRIVATE sherpa_ui sherpa_uiplugin Qt6::Test Qt6::Quick Qt6::QuickControls2)
add_test(NAME live COMMAND tst_live)
set_tests_properties(live PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 300)

add_executable(tst_daemon tst_daemon.cpp)
target_link_libraries(tst_daemon PRIVATE sherpa_ui Qt6::Test)
add_test(NAME daemon COMMAND tst_daemon)

# Тестам не нужен дисплей: платформа offscreen, если не задана своя (SHERPA_GL_TESTS=1 — реальный GL через xvfb)
foreach(t coreclient terminal daemon)
    set_tests_properties(${t} PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
endforeach()

add_executable(tst_files tst_files.cpp)
target_link_libraries(tst_files PRIVATE sherpa_ui sherpa_uiplugin Qt6::Test Qt6::Quick Qt6::QuickControls2)
add_test(NAME files COMMAND tst_files)
set_tests_properties(files PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 400)

add_executable(tst_termapps tst_termapps.cpp)
target_link_libraries(tst_termapps PRIVATE sherpa_ui sherpa_uiplugin Qt6::Test Qt6::Quick Qt6::QuickControls2)
add_test(NAME termapps COMMAND tst_termapps)
set_tests_properties(termapps PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 300)

add_executable(tst_sysutils tst_sysutils.cpp)
target_link_libraries(tst_sysutils PRIVATE sherpa_ui sherpa_uiplugin Qt6::Test Qt6::Quick Qt6::QuickControls2)
add_test(NAME sysutils COMMAND tst_sysutils)
set_tests_properties(sysutils PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 600)

add_executable(tst_tools tst_tools.cpp)
target_link_libraries(tst_tools PRIVATE sherpa_ui sherpa_uiplugin Qt6::Test Qt6::Quick Qt6::QuickControls2)
add_test(NAME tools COMMAND tst_tools)
set_tests_properties(tools PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 600)
