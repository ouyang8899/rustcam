add_executable(rustcam-browser
    main.cpp
    BrowserApp.cpp
    BrowserApp.h
    IpcServer.cpp
    IpcServer.h
    InstanceProfile.cpp
    InstanceProfile.h
    OverlayWindow.cpp
    OverlayWindow.h
    RenderProcessBridge.h
    SessionClient.cpp
    SessionClient.h
    SessionManager.cpp
    SessionManager.h
)

target_include_directories(rustcam-browser PRIVATE
    ${CMAKE_SOURCE_DIR}
    ${CMAKE_SOURCE_DIR}/common
)

target_link_libraries(rustcam-browser PRIVATE
    cef
    libcef_dll_wrapper
)

if(Qt5_FOUND)
    target_link_libraries(rustcam-browser PRIVATE Qt5::Core Qt5::Network)
else()
    target_link_libraries(rustcam-browser PRIVATE Qt6::Core Qt6::Network)
endif()

if(UNIX AND NOT APPLE)
    find_package(X11)
    if(X11_FOUND)
        target_link_libraries(rustcam-browser PRIVATE ${X11_LIBRARIES})
        target_include_directories(rustcam-browser PRIVATE ${X11_INCLUDE_DIR})
    endif()
endif()

set_target_properties(rustcam-browser PROPERTIES
    OUTPUT_NAME "rustcam-browser"
    RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin/rustcam-browser
)

# Qt 运行时：浏览器进程只需要 Core 与 Network（QLocalServer/QLocalSocket）。
# clientapp 目录里的 Qt DLL 不在 browser 的搜索路径上，必须单独部署。
set(BROWSER_QT_RUNTIME_FILES "")
if(TARGET Qt5::Core)
    get_target_property(_qt_core_location Qt5::Core LOCATION)
    if(_qt_core_location)
        get_filename_component(_qt_bin_dir "${_qt_core_location}" DIRECTORY)
        if(WIN32)
            # Debug 构建需要带 'd' 后缀的 Qt DLL，与根 CMakeLists 的做法一致。
            list(APPEND BROWSER_QT_RUNTIME_FILES
                "${_qt_bin_dir}/Qt5Core.dll"
                "${_qt_bin_dir}/Qt5Cored.dll"
                "${_qt_bin_dir}/Qt5Network.dll"
                "${_qt_bin_dir}/Qt5Networkd.dll")
        else()
            list(APPEND BROWSER_QT_RUNTIME_FILES
                "${_qt_bin_dir}/libQt5Core.so.5"
                "${_qt_bin_dir}/libQt5Network.so.5")
        endif()
    endif()
endif()

# CEF 运行时文件与 clientapp 分开部署：clientapp 进程不再加载 libcef.dll。
#
# 用 POST_BUILD 而不是独立的 custom target：独立 target 会因为
# $<TARGET_FILE_DIR:rustcam-browser> 的隐含依赖与 rustcam-browser 形成环，
# POST_BUILD 则没有这个问题，且随目标构建自动执行。
add_custom_command(TARGET rustcam-browser POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E make_directory
        $<TARGET_FILE_DIR:rustcam-browser>/locales
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${CEF_RUNTIME_BIN_FILES} ${CEF_RUNTIME_RES_FILES}
        $<TARGET_FILE_DIR:rustcam-browser>
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${CEF_RES_DIR}/locales/en-US.pak
        $<TARGET_FILE_DIR:rustcam-browser>/locales/
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${CEF_RES_DIR}/locales/zh-CN.pak
        $<TARGET_FILE_DIR:rustcam-browser>/locales/
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        ${BROWSER_QT_RUNTIME_FILES}
        $<TARGET_FILE_DIR:rustcam-browser>
    COMMENT "Deploying CEF and Qt runtime files for rustcam-browser"
)
