add_executable(map_page_test
    main.cpp
)

target_include_directories(map_page_test PRIVATE ${CMAKE_SOURCE_DIR})

target_link_libraries(map_page_test PRIVATE
    cef
    libcef_dll_wrapper
)

if(Qt5_FOUND)
    target_link_libraries(map_page_test PRIVATE Qt5::Core)
else()
    target_link_libraries(map_page_test PRIVATE Qt6::Core)
endif()

set_target_properties(map_page_test PROPERTIES
    RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin
)

add_test(NAME map_page_test COMMAND map_page_test)
