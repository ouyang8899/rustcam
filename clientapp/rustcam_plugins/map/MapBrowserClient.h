#pragma once

#include "include/cef_client.h"
#include "include/cef_frame.h"
#include "include/cef_life_span_handler.h"
#include "include/cef_load_handler.h"
#include "include/cef_process_message.h"

namespace rustcam {

class CefBrowserWidget;

// Browser-process side CefClient for one map browser.
//
// Owns the CefBrowser reference; the widget only borrows it. The reference is
// released in OnBeforeClose() so that no CEF object outlives CefShutdown().
class MapBrowserClient : public CefClient,
                         public CefLifeSpanHandler,
                         public CefLoadHandler {
public:
    explicit MapBrowserClient(CefBrowserWidget* widget);
    ~MapBrowserClient() override;

    CefRefPtr<CefBrowser> browser() const { return m_browser; }

    // The widget is not owned by the client; it detaches itself before it is
    // destroyed.
    void setWidget(CefBrowserWidget* widget);

    // Requests a forced close; the browser reference is dropped in
    // OnBeforeClose().
    void closeBrowser();

    // CefLifeSpanHandler
    void OnAfterCreated(CefRefPtr<CefBrowser> browser) override;
    void OnBeforeClose(CefRefPtr<CefBrowser> browser) override;

    // CefLoadHandler
    void OnLoadEnd(CefRefPtr<CefBrowser> browser,
                   CefRefPtr<CefFrame> frame,
                   int httpStatusCode) override;

    // CefClient
    //
    // The base CefClient returns nullptr for all handler getters, so the
    // LifeSpan and Load handlers must be exposed explicitly. Without this
    // CEF never dispatches OnBeforeClose()/OnLoadEnd() to this client.
    CefRefPtr<CefLifeSpanHandler> GetLifeSpanHandler() override { return this; }
    CefRefPtr<CefLoadHandler> GetLoadHandler() override { return this; }

    bool OnProcessMessageReceived(CefRefPtr<CefBrowser> browser,
                                  CefRefPtr<CefFrame> frame,
                                  CefProcessId source_process,
                                  CefRefPtr<CefProcessMessage> message) override;

private:
    IMPLEMENT_REFCOUNTING(MapBrowserClient);

    CefRefPtr<CefBrowser> m_browser;
    CefBrowserWidget* m_widget = nullptr;
};

} // namespace rustcam
