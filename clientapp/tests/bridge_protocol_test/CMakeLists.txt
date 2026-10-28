add_executable(bridge_protocol_test
    main.cpp
)

target_include_directories(bridge_protocol_test PRIVATE ${CMAKE_SOURCE_DIR})

if(Qt5_FOUND)
    target_link_libraries(bridge_protocol_test PRIVATE Qt5::Core)
else()
    target_link_libraries(bridge_protocol_test PRIVATE Qt6::Core)
endif()

set_target_properties(bridge_protocol_test PROPERTIES
    RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin
)

add_test(NAME bridge_protocol_test COMMAND bridge_protocol_test)
