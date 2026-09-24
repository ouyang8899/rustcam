set(CORE_LIB_SOURCES
    App.cpp
    MainWindow.cpp
    EventBus.cpp
    ConfigManager.cpp
    ThemeManager.cpp
    PluginManager.cpp
    LeftNavigationBar.cpp
    SessionManager.cpp
    ServerConfig.cpp
    ServerApi.cpp
    LoginWindow.cpp
    ServerConfigDialog.cpp
    AnimatedBackground.cpp
    DeviceTree.cpp
    VideoPlayer.cpp
    VideoWidgetManager.cpp
    PlayerLayout.cpp
)

set(CORE_LIB_HEADERS
    App.h
    MainWindow.h
    EventBus.h
    ConfigManager.h
    ThemeManager.h
    PluginManager.h
    LeftNavigationBar.h
    SessionManager.h
    ServerConfig.h
    ServerApi.h
    LoginWindow.h
    ServerConfigDialog.h
    AnimatedBackground.h
    DeviceTree.h
    DeviceTreeNode.h
    VideoPlayer.h
    VideoWidgetManager.h
    PlayerLayout.h
)

add_library(rustcam-core SHARED
    ${CORE_LIB_SOURCES}
    ${CORE_LIB_HEADERS}
)

target_compile_definitions(rustcam-core PRIVATE RUSTCAM_CORE_EXPORTS)

target_include_directories(rustcam-core PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}
    ${CMAKE_SOURCE_DIR}/common
    ${CMAKE_SOURCE_DIR}/interfaces
    ${CMAKE_SOURCE_DIR}
)

target_link_libraries(rustcam-core PUBLIC
    common
    interfaces
)

if(Qt5_FOUND)
    target_link_libraries(rustcam-core PUBLIC
        Qt5::Core
        Qt5::Widgets
        Qt5::Network
    )
else()
    target_link_libraries(rustcam-core PUBLIC
        Qt6::Core
        Qt6::Widgets
        Qt6::Network
    )
endif()

set(BOOST_ROOT "${CMAKE_SOURCE_DIR}/3rd_party/boost")
set(BOOST_INCLUDEDIR "${BOOST_ROOT}/include/boost-1_91")
set(BOOST_LIBRARYDIR "${BOOST_ROOT}/lib")
set(Boost_USE_STATIC_LIBS OFF)
set(Boost_USE_MULTITHREADED ON)
add_definitions(-DBOOST_ALL_NO_LIB)

set(CURL_ROOT "${CMAKE_SOURCE_DIR}/3rd_party/CURL")
set(CURL_INCLUDE_DIR "${CURL_ROOT}/include")
set(CURL_LIBRARY "${CURL_ROOT}/lib/libcurl_imp.lib")

set(OPENSSL_ROOT "${CMAKE_SOURCE_DIR}/3rd_party/openssl")
set(OPENSSL_INCLUDE_DIR "${OPENSSL_ROOT}/include")
set(OPENSSL_CRYPTO_LIBRARY "${OPENSSL_ROOT}/lib/libcrypto.lib")
set(OPENSSL_SSL_LIBRARY "${OPENSSL_ROOT}/lib/libssl.lib")

set(SQLITE3_ROOT "${CMAKE_SOURCE_DIR}/3rd_party/SQLITE3")
set(SQLITE3_INCLUDE_DIR "${SQLITE3_ROOT}/include")
set(SQLITE3_LIBRARY "${SQLITE3_ROOT}/lib/sqlite3.lib")

set(JSONCPP_ROOT "${CMAKE_SOURCE_DIR}/3rd_party/jsoncpp")
set(JSONCPP_INCLUDE_DIR "${JSONCPP_ROOT}/include")
set(JSONCPP_LIBRARY "${JSONCPP_ROOT}/lib/jsoncpp.lib")

target_include_directories(rustcam-core PRIVATE
    ${CURL_INCLUDE_DIR}
    ${OPENSSL_INCLUDE_DIR}
    ${SQLITE3_INCLUDE_DIR}
    ${FFMPEG_INCLUDE_DIR}
    ${BOOST_INCLUDEDIR}
    ${CMAKE_SOURCE_DIR}/3rd_party
    ${BRIDGE_PLAYER_INCLUDE_DIR}
    ${JSONCPP_INCLUDE_DIR}
)

target_link_libraries(rustcam-core PRIVATE
    ${CURL_LIBRARY}
    ${OPENSSL_SSL_LIBRARY}
    ${OPENSSL_CRYPTO_LIBRARY}
    ${SQLITE3_LIBRARY}
    ${JSONCPP_LIBRARY}
    bridge_player
)

set_target_properties(rustcam-core PROPERTIES
    OUTPUT_NAME "rustcam-core"
)

add_executable(rustcam-client
    main.cpp
)

target_include_directories(rustcam-client PRIVATE
    ${CMAKE_SOURCE_DIR}/3rd_party
    ${BRIDGE_PLAYER_INCLUDE_DIR}
    ${CURL_INCLUDE_DIR}
    ${FFMPEG_INCLUDE_DIR}
)

target_link_libraries(rustcam-client PRIVATE
    rustcam-core
    ${CURL_LIBRARY}
    bridge_player
)

set_target_properties(rustcam-client PROPERTIES
    WIN32_EXECUTABLE ON
    OUTPUT_NAME "rustcam-client"
)

add_custom_command(TARGET rustcam-client POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E copy_directory
        ${FFMPEG_DIR}/bin
        $<TARGET_FILE_DIR:rustcam-client>
    COMMAND ${CMAKE_COMMAND} -E copy
        ${BRIDGE_PLAYER_BIN_DIR}/bridge_player.dll
        $<TARGET_FILE_DIR:rustcam-client>
    COMMENT "Copying ffmpeg and bridge_player DLLs to output directory"
)
