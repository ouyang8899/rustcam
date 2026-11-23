#include "SessionClient.h"
#include "SessionManager.h"
#include "OverlayWindow.h"

#include "include/cef_browser.h"

#include "common/CefBridgeProtocol.h"

#include <cstdio>

namespace rustcam {
namespace browser {

SessionClient::SessionClient(SessionManager* manager, QString sessionId)
    : m_manager(manager)
    , m_sessionId(std::move(sessionId))
{
}

void SessionClient::OnAfterCreated(CefRefPtr<CefBrowser> browser)
{
    m_browser = browser;
    std::fprintf(stderr, "[SessionClient] OnAfterCreated id=%s hwnd=%llu\n",
                 m_sessionId.toUtf8().constData(),
                 static_cast<unsigned long long>(
                     toPortableHandle(browser->GetHost()->GetWindowHandle())));
    std::fflush(stderr);
    if (m_manager) {
        m_manager->onSessionCreated(m_sessionId,
                                    toPortableHandle(browser->GetHost()->GetWindowHandle()));
    }
}

void SessionClient::OnBeforeClose(CefRefPtr<CefBrowser> /*browser*/)
{
    m_browser = nullptr;
    if (m_manager) {
        m_manager->onSessionClosed(m_sessionId);
    }
}

void SessionClient::OnLoadEnd(CefRefPtr<CefBrowser> /*browser*/,
                              CefRefPtr<CefFrame> /*frame*/,
                              int /*httpStatusCode*/)
{
    std::fprintf(stderr, "[SessionClient] OnLoadEnd id=%s\n",
                 m_sessionId.toUtf8().constData());
    std::fflush(stderr);
    if (m_manager) {
        m_manager->onSessionReady(m_sessionId);
    }
}

bool SessionClient::OnProcessMessageReceived(CefRefPtr<CefBrowser> /*browser*/,
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
    // 载荷原样透传给 clientapp（spec 规则 R4）。
    const QString payload = QString::fromStdString(args->GetString(0).ToString());
    if (m_manager) {
        m_manager->onSessionEvent(m_sessionId, payload);
    }
    return true;
}

void SessionClient::executeJavaScript(const QString& script)
{
    CefRefPtr<CefBrowser> browser = m_browser;
    if (!browser) {
        return;
    }
    CefRefPtr<CefFrame> frame = browser->GetMainFrame();
    if (!frame) {
        return;
    }
    frame->ExecuteJavaScript(script.toStdString(), frame->GetURL().ToString(), 0);
}

void SessionClient::closeBrowser()
{
    CefRefPtr<CefBrowser> browser = m_browser;
    if (!browser) {
        return;
    }
    browser->GetHost()->CloseBrowser(true);
}

} // namespace browser
} // namespace rustcam
