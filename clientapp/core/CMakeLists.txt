set(CORE_SOURCES
    main.cpp
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
)

set(CORE_HEADERS
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

set(BOOST_ROOT "${CMAKE_SOURCE_DIR}/3rd_party/boost")
set(BOOST_INCLUDEDIR "${BOOST_ROOT}/include")
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

target_include_directories(rustcam-client PRIVATE
    ${CURL_INCLUDE_DIR}
    ${OPENSSL_INCLUDE_DIR}
    ${SQLITE3_INCLUDE_DIR}
)

target_link_libraries(rustcam-client PRIVATE
    ${CURL_LIBRARY}
    ${OPENSSL_SSL_LIBRARY}
    ${OPENSSL_CRYPTO_LIBRARY}
    ${SQLITE3_LIBRARY}
)

set_target_properties(rustcam-client PROPERTIES
    WIN32_EXECUTABLE ON
    OUTPUT_NAME "rustcam-client"
)
