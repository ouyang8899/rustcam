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

private:
    IMPLEMENT_REFCOUNTING(BrowserApp);
};
