set(PLUGIN_ID "device_tree")

set(PLUGIN_SOURCES
    DeviceTreePlugin.cpp
)

set(PLUGIN_HEADERS
    DeviceTreePlugin.h
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
    Qt5::Core
    Qt5::Widgets
)

set_target_properties(${PLUGIN_ID} PROPERTIES
    LIBRARY_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin/rustcam_plugins
    OUTPUT_NAME "rustcam_${PLUGIN_ID}"
    AUTOMOC ON
)
