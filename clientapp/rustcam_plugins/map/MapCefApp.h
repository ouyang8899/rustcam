#pragma once

#include "include/cef_app.h"
#include "include/cef_browser_process_handler.h"
#include "include/cef_command_line.h"

namespace rustcam {

// Browser-process side CefApp for the map plugin.
class MapCefApp : public CefApp, public CefBrowserProcessHandler {
public:
    MapCefApp() = default;

    CefRefPtr<CefBrowserProcessHandler> GetBrowserProcessHandler() override
    {
        return this;
    }

    void OnContextInitialized() override
    {
        // Reserved for future per-context wiring (permissions, schemes).
    }

    void OnBeforeCommandLineProcessing(const CefString& process_type,
                                        CefRefPtr<CefCommandLine> command_line) override
    {
        if (process_type.empty()) {
            // The GPU subprocess crashes on this machine (0xC0000003), which
            // leaves the page without any WebGL context. maplibre-gl v4
            // requires WebGL2, so the GPU service is hosted in the browser
            // process instead.
            command_line->AppendSwitch("in-process-gpu");
        }
    }

private:
    // Include the default reference counting implementation.
    IMPLEMENT_REFCOUNTING(MapCefApp);
};

} // namespace rustcam
