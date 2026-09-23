add_library(common STATIC
    Logger.cpp
    Utils.cpp
    QtVersionCompat.h
    Types.h
    StyledWidget.h
    ThemeStyle.h
)

target_include_directories(common PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}
)

if(Qt5_FOUND)
    target_link_libraries(common Qt5::Core Qt5::Widgets Qt5::Network)
else()
    target_link_libraries(common Qt6::Core Qt6::Widgets Qt6::Network)
endif()

set_target_properties(common PROPERTIES
    POSITION_INDEPENDENT_CODE ON
    CXX_VISIBILITY_PRESET hidden
)
