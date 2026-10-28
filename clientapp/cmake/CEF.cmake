# ---------------------------------------------------------------------------
# CEF (Chromium Embedded Framework) integration.
#
# Distribution layout:
#   <CEF_ROOT>/include       public headers (C API + C++ wrapper headers)
#   <CEF_ROOT>/libcef_dll   C++ wrapper sources, compiled into a static lib
#   <CEF_ROOT>/Release      libcef.dll + libcef.lib (import library)
#   <CEF_ROOT>/Resources    icudtl.dat, *.pak, locales/
# ---------------------------------------------------------------------------

set(CEF_ROOT "${THIRD_PARTY_DIR}/cef_binary_156.0.14+g4fa4ab5+chromium-156.0.8078.48_windows64"
    CACHE PATH "CEF binary distribution root")

if(NOT EXISTS "${CEF_ROOT}/include/cef_app.h")
    message(FATAL_ERROR "CEF distribution not found under ${CEF_ROOT}")
endif()

# We always ship the Release build of libcef.dll, in every build configuration.
set(CEF_BIN_DIR "${CEF_ROOT}/Release")
set(CEF_RES_DIR "${CEF_ROOT}/Resources")

# --- shared compile settings ------------------------------------------------
set(CEF_COMMON_DEFINES
    WIN32 _WIN32 _WINDOWS
    UNICODE _UNICODE
    WINVER=0x0A00
    _WIN32_WINNT=0x0A00
    NTDDI_VERSION=NTDDI_WIN10_FE
    NOMINMAX
    WIN32_LEAN_AND_MEAN
    __STDC_CONSTANT_MACROS
    __STDC_FORMAT_MACROS
)

if(MSVC)
    set(CEF_COMMON_OPTIONS
        /W4 /wd4100 /wd4127 /wd4244 /wd4324 /wd4481 /wd4512 /wd4701 /wd4702 /wd4996 /utf-8)
else()
    set(CEF_COMMON_OPTIONS "")
endif()

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
target_link_libraries(cef INTERFACE
    "${CEF_BIN_DIR}/libcef.lib"
    comctl32 crypt32 gdi32 rpcrt4 shlwapi wintrust ws2_32
)
