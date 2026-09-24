add_library(interfaces SHARED
    IPlugin.h
    IPlugin.cpp
    InterfacesExport.h
)

target_compile_definitions(interfaces PRIVATE INTERFACES_EXPORTS)

target_include_directories(interfaces PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}
)

if(Qt5_FOUND)
    target_link_libraries(interfaces PUBLIC Qt5::Core Qt5::Widgets)
else()
    target_link_libraries(interfaces PUBLIC Qt6::Core Qt6::Widgets)
endif()

set_target_properties(interfaces PROPERTIES
    OUTPUT_NAME "rustcam-interfaces"
    RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin
    LIBRARY_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin
    POSITION_INDEPENDENT_CODE ON
)
