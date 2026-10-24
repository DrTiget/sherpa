add_executable(tst_coreclient tst_coreclient.cpp)
target_link_libraries(tst_coreclient PRIVATE sherpa_ui Qt6::Test)
add_test(NAME coreclient COMMAND tst_coreclient)

add_executable(tst_screens tst_screens.cpp)
target_link_libraries(tst_screens PRIVATE sherpa_ui sherpa_uiplugin Qt6::Test Qt6::Quick Qt6::QuickControls2)
add_test(NAME screens COMMAND tst_screens)
set_tests_properties(screens PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
