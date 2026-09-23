set(PLUGIN_ID "live_video")

set(PLUGIN_SOURCES
    LiveVideoPlugin.cpp
)

set(PLUGIN_HEADERS
    LiveVideoPlugin.h
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
