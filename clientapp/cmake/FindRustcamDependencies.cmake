find_program(CCMAKE NAMES cmake)
find_program(CMAKECOMMAND NAMES cmake)
find_program(NINJA NAMES ninja)
find_program(QMAKE NAMES qmake)

execute_process(
    COMMAND ${CCMAKE} --version
    OUTPUT_VARIABLE CMAKE_VERSION
    OUTPUT_STRIP_TRAILING_WHITESPACE
)

message(STATUS "CMake version: ${CMAKE_VERSION}")
