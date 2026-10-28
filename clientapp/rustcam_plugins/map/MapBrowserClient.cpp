#include "MapBrowserClient.h"
#include "CefBrowserWidget.h"
#include "CefRuntime.h"
#include "common/CefBridgeProtocol.h"
#include "common/Logger.h"

#include "include/base/cef_bind.h"
#include "include/base/cef_callback.h"
#include "include/cef_task.h"
#include "include/wrapper/cef_closure_task.h"

#include <QMetaObject>

namespace rustcam {

MapBrowserClient::MapBrowserClient(CefBrowserWidget* widget)
    : m_widget(widget)
{
}

MapBrowserClient::~MapBrowserClient() = default;

void MapBrowserClient::setWidget(CefBrowserWidget* widget)
{
    m_widget = widget;
}

void MapBrowserClient::closeBrowser()
{
    if (!m_browser) {
        return;
    }
    CefRefPtr<CefBrowser> browser = m_browser;
    // CloseBrowser() must run on the CEF UI thread.
    CefPostTask(TID_UI, base::BindOnce(
        [](CefRefPtr<CefBrowser> browser) {
            if (browser) {
                browser->GetHost()->CloseBrowser(true);
            }
        },
        browser));
}

void MapBrowserClient::OnAfterCreated(CefRefPtr<CefBrowser> browser)
{
    // Keep the browser reference here: ExecuteJavaScript() and closeBrowser()
    // need it, and CEF does not hand it to us anywhere else.
    m_browser = browser;
}

void MapBrowserClient::OnBeforeClose(CefRefPtr<CefBrowser> /*browser*/)
{
    m_browser = nullptr;
    CefRuntime::instance().removeClient(this);
}

void MapBrowserClient::OnLoadEnd(CefRefPtr<CefBrowser> /*browser*/,
                                 CefRefPtr<CefFrame> /*frame*/,
                                 int /*httpStatusCode*/)
{
    if (!m_widget) {
        return;
    }
    // The functor overload of invokeMethod() takes the widget as context
    // object: the queued event is dropped if the widget is destroyed first.
    QMetaObject::invokeMethod(m_widget, [w = m_widget]() {
        w->handleLoadEnd();
    }, Qt::QueuedConnection);
}

bool MapBrowserClient::OnProcessMessageReceived(CefRefPtr<CefBrowser> /*browser*/,
                                                 CefRefPtr<CefFrame> /*frame*/,
                                                 CefProcessId /*source_process*/,
                                                 CefRefPtr<CefProcessMessage> message)
{
    if (!message || message->GetName() != rustcam::cefbridge::kNativeMessageName) {
        return false;
    }

    CefRefPtr<CefListValue> args = message->GetArgumentList();
    if (!args || args->GetSize() < 1) {
        return true;
    }
    const QString payload = QString::fromStdString(args->GetString(0).ToString());

    if (!m_widget) {
        return true;
    }
    QMetaObject::invokeMethod(m_widget, [w = m_widget, payload]() {
        w->handleBridgeMessage(payload);
    }, Qt::QueuedConnection);
    return true;
}

} // namespace rustcam
