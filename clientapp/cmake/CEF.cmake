# ---------------------------------------------------------------------------
# CEF (Chromium Embedded Framework) integration.
#
# Distribution layout:
#   <CEF_ROOT>/include       public headers (C API + C++ wrapper headers)
#   <CEF_ROOT>/libcef_dll   C++ wrapper sources, compiled into a static lib
#   <CEF_ROOT>/Release      libcef.dll + libcef.lib (import library)
#   <CEF_ROOT>/Resources    icudtl.dat, *.pak, locales/
# ---------------------------------------------------------------------------

# CEF 发行版按平台选择。CEF_ROOT 仍是 cache 变量，显式指定时优先使用。
if(NOT CEF_ROOT)
    if(WIN32)
        set(CEF_PLATFORM_TAG "windows64")
    elseif(CMAKE_SYSTEM_NAME STREQUAL "Linux")
        set(CEF_PLATFORM_TAG "linux64")
    else()
        # CEF 没有 macOS 官方发行版，先按 Windows 兜底。
        set(CEF_PLATFORM_TAG "windows64")
    endif()
    set(CEF_ROOT "${THIRD_PARTY_DIR}/cef_binary_156.0.14+g4fa4ab5+chromium-156.0.8078.48_${CEF_PLATFORM_TAG}"
        CACHE PATH "CEF binary distribution root")
endif()

if(NOT EXISTS "${CEF_ROOT}/include/cef_app.h")
    message(FATAL_ERROR "CEF distribution not found under ${CEF_ROOT}")
endif()

# We always ship the Release build of libcef.dll, in every build configuration.
set(CEF_BIN_DIR "${CEF_ROOT}/Release")
set(CEF_RES_DIR "${CEF_ROOT}/Resources")

# --- shared compile settings ------------------------------------------------
# Windows 专属的宏只在 Windows 上定义；Linux 上带上它们会污染编译。
set(CEF_COMMON_DEFINES
    NOMINMAX
    __STDC_CONSTANT_MACROS
    __STDC_FORMAT_MACROS
)

if(WIN32)
    list(APPEND CEF_COMMON_DEFINES
        WIN32 _WIN32 _WINDOWS
        UNICODE _UNICODE
        WINVER=0x0A00
        _WIN32_WINNT=0x0A00
        NTDDI_VERSION=NTDDI_WIN10_FE
        WIN32_LEAN_AND_MEAN
    )
endif()

if(MSVC)
    set(CEF_COMMON_OPTIONS
        /W4 /wd4100 /wd4127 /wd4244 /wd4324 /wd4481 /wd4512 /wd4701 /wd4702 /wd4996 /utf-8)
else()
    set(CEF_COMMON_OPTIONS "")
endif()

# --- runtime deployment ------------------------------------------------------
# CEF 运行时文件清单。定义在这里（而不是根 CMakeLists），因为
# clientapp/browser/ 子目录在根 CMakeLists 的 runtime 段落之前就被展开，
# 放到那里会拿到空的变量。
#
# 按平台挑选（Windows 是 .dll，Linux 是 .so）。
if(WIN32)
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
else()
    set(CEF_RUNTIME_BIN_FILES
        ${CEF_BIN_DIR}/libcef.so
        ${CEF_BIN_DIR}/chrome-sandbox
        ${CEF_BIN_DIR}/v8_context_snapshot.bin
        ${CEF_BIN_DIR}/snapshot_blob.bin
        ${CEF_BIN_DIR}/libvk_swiftshader.so
        ${CEF_BIN_DIR}/libEGL.so
        ${CEF_BIN_DIR}/libGLESv2.so
    )
endif()

set(CEF_RUNTIME_RES_FILES
    ${CEF_RES_DIR}/icudtl.dat
    ${CEF_RES_DIR}/chrome_100_percent.pak
    ${CEF_RES_DIR}/chrome_200_percent.pak
    ${CEF_RES_DIR}/resources.pak
)

# --- libcef_dll_wrapper (STATIC) --------------------------------------------
# The binary distribution ships the C++ wrapper as source; it must be compiled
# into the application.
file(GLOB_RECURSE CEF_WRAPPER_SOURCES CONFIGURE_DEPENDS "${CEF_ROOT}/libcef_dll/*.cc")

add_library(libcef_dll_wrapper STATIC ${CEF_WRAPPER_SOURCES})
target_include_directories(libcef_dll_wrapper PUBLIC "${CEF_ROOT}")
target_compile_features(libcef_dll_wrapper PUBLIC cxx_std_20)
target_compile_definitions(libcef_dll_wrapper PRIVATE
    ${CEF_COMMON_DEFINES}
    WRAPPING_CEF_SHARED)
if(MSVC)
    target_compile_options(libcef_dll_wrapper PRIVATE ${CEF_COMMON_OPTIONS} /MP /Gy /GF)
endif()

# --- cef (INTERFACE target for application code) ----------------------------
add_library(cef INTERFACE)
target_include_directories(cef INTERFACE "${CEF_ROOT}")
target_compile_features(cef INTERFACE cxx_std_20)
target_compile_definitions(cef INTERFACE ${CEF_COMMON_DEFINES})
if(MSVC)
    target_compile_options(cef INTERFACE ${CEF_COMMON_OPTIONS})
endif()

# 链接库按平台：Windows 用 import lib + Win32 库；Linux 用 .so。
if(WIN32)
    target_link_libraries(cef INTERFACE
        "${CEF_BIN_DIR}/libcef.lib"
        comctl32 crypt32 gdi32 rpcrt4 shlwapi wintrust ws2_32
    )
else()
    target_link_libraries(cef INTERFACE
        "${CEF_BIN_DIR}/libcef.so"
        dl pthread
    )
endif()
