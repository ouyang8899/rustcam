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
    SystemConfig.cpp
    ServerApi.cpp
    LoginWindow.cpp
    ServerConfigDialog.cpp
    AnimatedBackground.cpp
    DeviceTree.cpp
    DeviceTreeModel.cpp
    VideoPlayer.cpp
    VideoPlayerToolbar.cpp
    VideoWidgetManager.cpp
    PlayerLayout.cpp
    NotificationWidget.cpp
    DataManager.cpp
    ReplayPlayer.cpp
    StreamSource.cpp
    PlaybackController.cpp
    PlaybackCell.cpp
    PlaybackPlayerWidget.cpp
    ReplayPlayerContainer.cpp
    ReplayNavBar.cpp
    TimelineWidget.cpp
    RecordingFileListWidget.cpp
    FileDownloader.cpp
    BookmarkStore.cpp
    ClipExporter.cpp
    ClipExportDialog.cpp
    RecordingApi.cpp
    PlaybackChannel.cpp
    PlaybackSidePanel.cpp
    MapView.cpp
    MapSidePanel.cpp
    AlarmSidePanel.cpp
    PTZControlPanel.cpp
    PTZControlPanel.ui
)

set(CORE_LIB_HEADERS
    App.h
    MainWindow.h
    EventBus.h
    ConfigManager.h
    ConfigKeys.h
    ThemeManager.h
    PluginManager.h
    LeftNavigationBar.h
    SessionManager.h
    ServerConfig.h
    SystemConfig.h
    ServerApi.h
    LoginWindow.h
    ServerConfigDialog.h
    AnimatedBackground.h
    DeviceTree.h
    DeviceTreeModel.h
    DeviceTreeNode.h
    VideoPlayer.h
    VideoPlayerToolbar.h
    VideoWidgetManager.h
    PlayerLayout.h
    NotificationWidget.h
    DataManager.h
    ReplayPlayer.h
    StreamSource.h
    PlaybackController.h
    PlaybackCell.h
    PlaybackPlayerWidget.h
    ReplayPlayerContainer.h
    ReplayNavBar.h
    TimelineWidget.h
    RecordingFileListWidget.h
    FileDownloader.h
    BookmarkStore.h
    ClipExporter.h
    ClipExportDialog.h
    RecordingTypes.h
    RecordingApi.h
    PlaybackChannel.h
    PlaybackSidePanel.h
    MapView.h
    MapSidePanel.h
    AlarmSidePanel.h
    PTZControlPanel.h
)

add_library(rustcam-core SHARED
    ${CORE_LIB_SOURCES}
    ${CORE_LIB_HEADERS}
    icons/icons.qrc
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
        Qt5::Svg
        Qt5::Concurrent
    )
    if(Qt5_WebEngineWidgets_FOUND)
        target_link_libraries(rustcam-core PUBLIC Qt5::WebEngineWidgets)
    endif()
else()
    target_link_libraries(rustcam-core PUBLIC
        Qt6::Core
        Qt6::Widgets
        Qt6::Network
        Qt6::Svg
        Qt6::Concurrent
    )
    if(Qt6_WebEngineWidgets_FOUND)
        target_link_libraries(rustcam-core PUBLIC Qt6::WebEngineWidgets)
    endif()
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
    ${FFMPEG_LIB_DIR}/avformat.lib
    ${FFMPEG_LIB_DIR}/avcodec.lib
    ${FFMPEG_LIB_DIR}/avutil.lib
    ${FFMPEG_LIB_DIR}/swscale.lib
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
    COMMAND ${CMAKE_COMMAND} -E copy
        ${CMAKE_SOURCE_DIR}/3rd_party/jsoncpp/bin/jsoncpp.dll
        $<TARGET_FILE_DIR:rustcam-client>
    COMMAND ${CMAKE_COMMAND} -E copy
        ${BRIDGE_PLAYER_BIN_DIR}/SDL3.dll
        $<TARGET_FILE_DIR:rustcam-client>
    COMMENT "Copying ffmpeg and bridge_player DLLs to output directory"
)
