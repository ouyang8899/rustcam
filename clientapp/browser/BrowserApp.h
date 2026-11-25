#pragma once

#include "include/cef_app.h"
#include "include/cef_browser_process_handler.h"
#include "include/cef_command_line.h"

// rustcam-browser 进程的 browser 进程侧 CefApp。
//
// 承接原 map 插件中 MapCefApp 的职责（clientapp/rustcam_plugins/map/MapCefApp.h）。
class BrowserApp : public CefApp, public CefBrowserProcessHandler {
public:
    BrowserApp() = default;

    CefRefPtr<CefBrowserProcessHandler> GetBrowserProcessHandler() override
    {
        return this;
    }

    void OnBeforeCommandLineProcessing(const CefString& process_type,
                                        CefRefPtr<CefCommandLine> command_line) override
    {
        if (process_type.empty()) {
            // GPU 子进程在部分机器上崩溃（0xC0000003），会导致页面没有
            // WebGL 上下文。maplibre-gl v4 需要 WebGL2，因此 GPU 服务宿主在
            // browser 进程内。此处沿用原 MapCefApp 的处理。
            command_line->AppendSwitch("in-process-gpu");
        }
    }

    /// 已有实例在跑（共用 profile）时，CEF 默认会把本次启动参数转发给那个
    /// 实例——参数里的 "--pipe rustcam-browser-<pid>" 会被当成 URL 打开，
    /// 用户看到一个凭空出现的 Chromium 窗口。这里接管掉：转发没有意义，
    /// 新进程本来也起不来。
    bool OnAlreadyRunningAppRelaunch(CefRefPtr<CefCommandLine> command_line,
                                     const CefString& current_directory) override
    {
        (void)command_line;
        (void)current_directory;
        return true;
    }

private:
    IMPLEMENT_REFCOUNTING(BrowserApp);
};
