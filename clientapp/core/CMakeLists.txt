set(CORE_SOURCES
    main.cpp
    App.cpp
    MainWindow.cpp
    EventBus.cpp
    ConfigManager.cpp
    ThemeManager.cpp
    PluginManager.cpp
    LeftNavigationBar.cpp
)

set(CORE_HEADERS
    App.h
    MainWindow.h
    EventBus.h
    ConfigManager.h
    ThemeManager.h
    PluginManager.h
    LeftNavigationBar.h
)

add_executable(rustcam-client
    ${CORE_SOURCES}
    ${CORE_HEADERS}
)

target_include_directories(rustcam-client PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}
    ${CMAKE_SOURCE_DIR}/common
    ${CMAKE_SOURCE_DIR}/interfaces
    ${CMAKE_SOURCE_DIR}
)

target_link_libraries(rustcam-client PRIVATE
    common
    interfaces
)

if(Qt5_FOUND)
    target_link_libraries(rustcam-client PRIVATE
        Qt5::Core
        Qt5::Widgets
        Qt5::Network
    )
else()
    target_link_libraries(rustcam-client PRIVATE
        Qt6::Core
        Qt6::Widgets
        Qt6::Network
    )
endif()

set_target_properties(rustcam-client PROPERTIES
    WIN32_EXECUTABLE ON
    OUTPUT_NAME "rustcam-client"
)
