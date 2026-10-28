add_executable(cef_smoke
    main.cpp
)

target_link_libraries(cef_smoke PRIVATE
    cef
    libcef_dll_wrapper
)

if(Qt5_FOUND)
    target_link_libraries(cef_smoke PRIVATE Qt5::Core)
else()
    target_link_libraries(cef_smoke PRIVATE Qt6::Core)
endif()

set_target_properties(cef_smoke PROPERTIES
    RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin
)

add_test(NAME cef_smoke COMMAND cef_smoke)
