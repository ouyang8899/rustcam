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

find_package(Qt5 COMPONENTS Core Widgets Network Concurrent Svg QUIET)
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

set(THIRD_PARTY_DIR ${CMAKE_SOURCE_DIR}/3rd_party)

set(FFMPEG_DIR ${THIRD_PARTY_DIR}/ffmpeg-n8.1.3-win64-lgpl-shared-8.1)
set(BRIDGE_PLAYER_DIR ${THIRD_PARTY_DIR}/bridge_player)

set(FFMPEG_INCLUDE_DIR ${FFMPEG_DIR}/include)
set(FFMPEG_LIB_DIR ${FFMPEG_DIR}/lib)
set(BRIDGE_PLAYER_INCLUDE_DIR ${BRIDGE_PLAYER_DIR}/include)
set(BRIDGE_PLAYER_LIB_DIR ${BRIDGE_PLAYER_DIR}/lib)
set(BRIDGE_PLAYER_BIN_DIR ${BRIDGE_PLAYER_DIR}/bin)

add_library(bridge_player SHARED IMPORTED)
set_target_properties(bridge_player PROPERTIES
    IMPORTED_LOCATION ${BRIDGE_PLAYER_BIN_DIR}/bridge_player.dll
    IMPORTED_IMPLIB ${BRIDGE_PLAYER_LIB_DIR}/bridge_player.lib
)

# --- CEF (Chromium Embedded Framework) ---------------------------------------
include(${CMAKE_SOURCE_DIR}/cmake/CEF.cmake)

enable_testing()
add_subdirectory(tests/cef_smoke)
add_subdirectory(tests/bridge_protocol_test)
add_subdirectory(tests/map_page_test)

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
file(MAKE_DIRECTORY ${CMAKE_BINARY_DIR}/bin/translations)
file(COPY ${CMAKE_SOURCE_DIR}/translations DESTINATION ${CMAKE_BINARY_DIR}/bin/)
file(COPY ${CMAKE_SOURCE_DIR}/rustcam_plugins/map/web
     DESTINATION ${CMAKE_BINARY_DIR}/bin/rustcam_plugins/map/)

find_program(LRELEASE_EXECUTABLE lrelease)
if(LRELEASE_EXECUTABLE)
    file(GLOB TS_FILES "${CMAKE_SOURCE_DIR}/translations/*.ts")
    foreach(TS_FILE ${TS_FILES})
        get_filename_component(QM_FILE ${TS_FILE} NAME_WE)
        set(QM_FILE "${CMAKE_BINARY_DIR}/bin/translations/${QM_FILE}.qm")
        add_custom_command(
            OUTPUT ${QM_FILE}
            COMMAND ${LRELEASE_EXECUTABLE} ${TS_FILE} -qm ${QM_FILE}
            DEPENDS ${TS_FILE}
            COMMENT "Compiling ${TS_FILE} to ${QM_FILE}"
        )
        list(APPEND QM_FILES ${QM_FILE})
    endforeach()
    if(QM_FILES)
        add_custom_target(translations ALL DEPENDS ${QM_FILES})
    endif()
else()
    message(STATUS "lrelease not found, translations will not be compiled")
endif()

file(MAKE_DIRECTORY ${CMAKE_BINARY_DIR}/bin/rustcam_plugins)

# --- runtime deployment ------------------------------------------------------
# Everything the executable needs at run time: CEF, third party libraries and
# the Qt runtime. Without these the build output cannot be started.
set(CEF_RUNTIME_BIN_FILES
    ${CEF_BIN_DIR}/libcef.dll
    ${CEF_BIN_DIR}/chrome_elf.dll
    ${CEF_BIN_DIR}/v8_context_snapshot.bin
    ${CEF_BIN_DIR}/vk_swiftshader.dll
    ${CEF_BIN_DIR}/vk_swiftshader_icd.json
    ${CEF_BIN_DIR}/vulkan-1.dll
    ${CEF_BIN_DIR}/d3dcompiler_47.dll
    ${CEF_BIN_DIR}/dxcompiler.dll
)

set(CEF_RUNTIME_RES_FILES
    ${CEF_RES_DIR}/icudtl.dat
    ${CEF_RES_DIR}/chrome_100_percent.pak
    ${CEF_RES_DIR}/chrome_200_percent.pak
    ${CEF_RES_DIR}/resources.pak
)

set(THIRD_PARTY_RUNTIME_FILES
    ${CMAKE_SOURCE_DIR}/3rd_party/CURL/bin/libcurl.dll
    ${CMAKE_SOURCE_DIR}/3rd_party/CURL/bin/libcurl_dp.dll
    ${CMAKE_SOURCE_DIR}/3rd_party/CURL/bin/libcrypto-4-x64.dll
    ${CMAKE_SOURCE_DIR}/3rd_party/CURL/bin/libssl-4-x64.dll
    ${CMAKE_SOURCE_DIR}/3rd_party/CURL/bin/libzstd.dll
    ${CMAKE_SOURCE_DIR}/3rd_party/SQLITE3/bin/sqlite3.dll
)

# Qt shared libraries. Both the release and the debug flavours are deployed
# so that the build output runs in every configuration. The location is
# derived from the imported Qt target because the imported location of a
# multi-config generator is not per configuration.
set(QT_RUNTIME_FILES "")
set(QT_RUNTIME_MODULES Core Gui Widgets Network Svg Concurrent)
if(TARGET Qt5::Core)
    get_target_property(_qt_core_location Qt5::Core LOCATION)
    if(_qt_core_location)
        get_filename_component(_qt_bin_dir "${_qt_core_location}" DIRECTORY)
        foreach(_qt_module ${QT_RUNTIME_MODULES})
            list(APPEND QT_RUNTIME_FILES "${_qt_bin_dir}/Qt5${_qt_module}.dll")
            list(APPEND QT_RUNTIME_FILES "${_qt_bin_dir}/Qt5${_qt_module}d.dll")
        endforeach()
    endif()
endif()

# The "platforms" plugin is required for QApplication to start. It lives next
# to the Qt libraries in <qt>/plugins/platforms.
set(QT_PLATFORM_FILES "")
if(TARGET Qt5::Core)
    get_target_property(_qt_core_location Qt5::Core LOCATION)
    if(_qt_core_location)
        get_filename_component(_qt_bin_dir "${_qt_core_location}" DIRECTORY)
        get_filename_component(_qt_root "${_qt_bin_dir}" DIRECTORY)
        set(QT_PLATFORM_FILES
            "${_qt_root}/plugins/platforms/qwindows.dll"
            "${_qt_root}/plugins/platforms/qwindowsd.dll")
    endif()
endif()

add_custom_target(runtime_files ALL
    COMMAND ${CMAKE_COMMAND} -E make_directory
        $<TARGET_FILE_DIR:rustcam-client>/locales
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${CEF_RUNTIME_BIN_FILES} ${CEF_RUNTIME_RES_FILES}
        $<TARGET_FILE_DIR:rustcam-client>
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${CEF_RES_DIR}/locales/en-US.pak
        $<TARGET_FILE_DIR:rustcam-client>/locales/
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${CEF_RES_DIR}/locales/zh-CN.pak
        $<TARGET_FILE_DIR:rustcam-client>/locales/
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${THIRD_PARTY_RUNTIME_FILES}
        $<TARGET_FILE_DIR:rustcam-client>
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${QT_RUNTIME_FILES}
        $<TARGET_FILE_DIR:rustcam-client>
    COMMENT "Deploying runtime files (CEF, third party, Qt)"
)

if(QT_PLATFORM_FILES)
    add_custom_command(TARGET runtime_files POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E make_directory
            $<TARGET_FILE_DIR:rustcam-client>/platforms
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
            ${QT_PLATFORM_FILES}
            $<TARGET_FILE_DIR:rustcam-client>/platforms/
    )
endif()

message(STATUS "=================================================")
message(STATUS "rustcam-client v${PROJECT_VERSION}")
message(STATUS "Qt Version: ${QT_VERSION_STRING} (Major: ${QT_VERSION_MAJOR})")
message(STATUS "Build Type: ${CMAKE_BUILD_TYPE}")
message(STATUS "Install Prefix: ${CMAKE_INSTALL_PREFIX}")
message(STATUS "FFmpeg: ${FFMPEG_DIR}")
message(STATUS "BridgePlayer: ${BRIDGE_PLAYER_DIR}")
message(STATUS "=================================================")
