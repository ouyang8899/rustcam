set(PLUGIN_ID "alarm")

set(PLUGIN_SOURCES
    AlarmPlugin.cpp
)

set(PLUGIN_HEADERS
    AlarmPlugin.h
)

add_library(${PLUGIN_ID} SHARED
    ${PLUGIN_SOURCES}
    ${PLUGIN_HEADERS}
    plugin.json
)

add_dependencies(${PLUGIN_ID} interfaces rustcam-core)

target_include_directories(${PLUGIN_ID} PRIVATE
    ${CMAKE_SOURCE_DIR}
    ${CMAKE_SOURCE_DIR}/core
    ${CMAKE_SOURCE_DIR}/interfaces
    ${CMAKE_SOURCE_DIR}/common
)

target_link_libraries(${PLUGIN_ID} PUBLIC
    interfaces
    common
    rustcam-core
    Qt5::Core
    Qt5::Widgets
)

set_target_properties(${PLUGIN_ID} PROPERTIES
    OUTPUT_NAME "rustcam-${PLUGIN_ID}"
    AUTOMOC ON
)

add_custom_command(TARGET ${PLUGIN_ID} POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E make_directory
        $<TARGET_FILE_DIR:${PLUGIN_ID}>/rustcam_plugins/${PLUGIN_ID}/icons/
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        $<TARGET_FILE:${PLUGIN_ID}>
        $<TARGET_FILE_DIR:${PLUGIN_ID}>/rustcam_plugins/${PLUGIN_ID}/
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${CMAKE_CURRENT_SOURCE_DIR}/plugin.json
        $<TARGET_FILE_DIR:${PLUGIN_ID}>/rustcam_plugins/${PLUGIN_ID}/
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${CMAKE_CURRENT_SOURCE_DIR}/icons/icon.svg
        $<TARGET_FILE_DIR:${PLUGIN_ID}>/rustcam_plugins/${PLUGIN_ID}/icons/
)
