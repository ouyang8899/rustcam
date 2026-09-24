set(PLUGIN_ID "live_video")

set(PLUGIN_SOURCES
    LiveVideoPlugin.cpp
    NavigationBar.cpp
    VideoPlayerContainer.cpp
)

set(PLUGIN_HEADERS
    LiveVideoPlugin.h
    NavigationBar.h
    VideoPlayerContainer.h
)

add_library(${PLUGIN_ID} SHARED
    ${PLUGIN_SOURCES}
    ${PLUGIN_HEADERS}
    plugin.json
)

target_include_directories(${PLUGIN_ID} PRIVATE
    ${CMAKE_SOURCE_DIR}
    ${CMAKE_SOURCE_DIR}/core
    ${CMAKE_SOURCE_DIR}/interfaces
    ${CMAKE_SOURCE_DIR}/common
    ${CMAKE_SOURCE_DIR}/3rd_party/bridge_player/include
    ${CMAKE_SOURCE_DIR}/3rd_party/boost/include/boost-1_91
    ${CMAKE_SOURCE_DIR}/3rd_party/ffmpeg-n8.1.3-win64-lgpl-shared-8.1/include
    ${CMAKE_SOURCE_DIR}/3rd_party/jsoncpp/include
)

target_link_libraries(${PLUGIN_ID} PRIVATE
    interfaces
    common
    rustcam-core
    Qt5::Core
    Qt5::Widgets
    Qt5::Concurrent
)

set_target_properties(${PLUGIN_ID} PROPERTIES
    LIBRARY_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin/rustcam_plugins
    OUTPUT_NAME "rustcam_${PLUGIN_ID}"
    AUTOMOC ON
)

add_custom_command(TARGET ${PLUGIN_ID} POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E copy
        $<TARGET_FILE:${PLUGIN_ID}>
        ${CMAKE_BINARY_DIR}/bin/rustcam_plugins/
)
add_custom_command(TARGET ${PLUGIN_ID} POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E copy
        $<TARGET_FILE:${PLUGIN_ID}>
        ${CMAKE_BINARY_DIR}/bin/Debug/rustcam_plugins/
)
