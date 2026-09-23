add_library(interfaces SHARED
    IPlugin.h
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
    POSITION_INDEPENDENT_CODE ON
)
