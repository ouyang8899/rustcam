add_library(interfaces INTERFACE)

target_include_directories(interfaces INTERFACE
    ${CMAKE_CURRENT_SOURCE_DIR}
)

if(Qt5_FOUND)
    target_link_libraries(interfaces INTERFACE Qt5::Core Qt5::Widgets)
else()
    target_link_libraries(interfaces INTERFACE Qt6::Core Qt6::Widgets)
endif()

set_target_properties(interfaces PROPERTIES
    POSITION_INDEPENDENT_CODE ON
)
