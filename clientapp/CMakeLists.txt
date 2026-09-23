cmake_minimum_required(VERSION 3.16)

project(rustcam-client VERSION 1.0.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(MSVC)
    add_compile_options(/utf-8)
endif()

set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
set(CMAKE_AUTOUIC ON)
set(CMAKE_INCLUDE_CURRENT_DIR ON)

if(POLICY CMP0143)
    cmake_policy(SET CMP0143 NEW)
endif()

find_package(Qt5 COMPONENTS Core Widgets Network Concurrent QUIET)
if(Qt5_FOUND)
    set(QT_VERSION_MAJOR 5)
    set(QT_VERSION_STRING ${Qt5_VERSION})
    message(STATUS "Detected Qt5: ${Qt5_VERSION}")
else()
    find_package(Qt6 COMPONENTS Core Widgets Network Concurrent QUIET)
    if(Qt6_FOUND)
        set(QT_VERSION_MAJOR 6)
        set(QT_VERSION_STRING ${Qt6_VERSION})
        message(STATUS "Detected Qt6: ${Qt6_VERSION}")
        set(CMAKE_CXX_STANDARD 20)
    else()
        message(FATAL_ERROR "Neither Qt5 nor Qt6 found. Please install Qt 5.15 or Qt 6.2+")
    endif()
endif()

set(RUSTCAM_QT_VERSION_MAJOR ${QT_VERSION_MAJOR})
set(RUSTCAM_QT_VERSION_STRING ${QT_VERSION_STRING})

if(QT_VERSION_MAJOR EQUAL 6)
    set(QT_COMPONENTS Core Widgets Network Concurrent)
    set(QT_DISABLE_DEPRECATED_BEFORE 0x060300)
else()
    set(QT_COMPONENTS Core Widgets Network Concurrent)
    set(QT_DISABLE_DEPRECATED_BEFORE 0x051500)
endif()

option(ENABLE_COVERAGE "Enable code coverage" OFF)

if(ENABLE_COVERAGE)
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} --coverage")
    set(CMAKE_EXE_LINKER_FLAGS "--coverage")
endif()

include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin)
set(CMAKE_LIBRARY_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/lib)
set(CMAKE_ARCHIVE_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/lib)

add_subdirectory(common)
add_subdirectory(core)
add_subdirectory(interfaces)
add_subdirectory(resources)
add_subdirectory(rustcam_plugins)

set(CONFIG_TEMPLATE "resources/config/default.json.in")
configure_file(${CONFIG_TEMPLATE} ${CMAKE_BINARY_DIR}/bin/config/default.json @ESCAPE)

file(COPY ${CMAKE_SOURCE_DIR}/resources/qss DESTINATION ${CMAKE_BINARY_DIR}/bin/resources/)
file(COPY ${CMAKE_SOURCE_DIR}/resources/icons DESTINATION ${CMAKE_BINARY_DIR}/bin/resources/)
file(COPY ${CMAKE_SOURCE_DIR}/resources/fonts DESTINATION ${CMAKE_BINARY_DIR}/bin/resources/)
file(MAKE_DIRECTORY ${CMAKE_BINARY_DIR}/bin/rustcam_plugins)

message(STATUS "=================================================")
message(STATUS "rustcam-client v${PROJECT_VERSION}")
message(STATUS "Qt Version: ${QT_VERSION_STRING} (Major: ${QT_VERSION_MAJOR})")
message(STATUS "Build Type: ${CMAKE_BUILD_TYPE}")
message(STATUS "Install Prefix: ${CMAKE_INSTALL_PREFIX}")
message(STATUS "=================================================")
