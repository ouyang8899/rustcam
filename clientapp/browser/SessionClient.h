#pragma once

#include "include/cef_client.h"
#include "include/cef_frame.h"
#include "include/cef_life_span_handler.h"
#include "include/cef_load_handler.h"
#include "include/cef_process_message.h"

#include <QString>

namespace rustcam {
namespace browser {

class SessionManager;

/// 一个 CEF browser 的客户端。
///
/// 负责三件事：
///   1. 页面加载完成（CefLoadHandler::OnLoadEnd）→ 通知 SessionManager 转发 session.ready
///   2. 渲染进程发来的 JS 消息（CefClient::OnProcessMessageReceived）→ 转发 session.event
///   3. browser 关闭（CefLifeSpanHandler::OnBeforeClose）→ 通知 SessionManager 移除会话
///
/// CEF 156 勘误：OnAfterCreated / OnBeforeClose 属于 CefLifeSpanHandler，
/// OnLoadEnd 属于 CefLoadHandler，**不再**是 CefClient 的虚函数。必须通过
/// GetLifeSpanHandler() / GetLoadHandler() 显式暴露，否则 CEF 不会派发。
class SessionClient : public CefClient,
                      public CefLifeSpanHandler,
                      public CefLoadHandler {
public:
    SessionClient(SessionManager* manager, QString sessionId);

    // CefLifeSpanHandler
    void OnAfterCreated(CefRefPtr<CefBrowser> browser) override;
    void OnBeforeClose(CefRefPtr<CefBrowser> browser) override;

    // CefLoadHandler
    void OnLoadEnd(CefRefPtr<CefBrowser> browser,
                   CefRefPtr<CefFrame> frame,
                   int httpStatusCode) override;

    // CefClient
    CefRefPtr<CefLifeSpanHandler> GetLifeSpanHandler() override { return this; }
    CefRefPtr<CefLoadHandler> GetLoadHandler() override { return this; }

    bool OnProcessMessageReceived(CefRefPtr<CefBrowser> browser,
                                  CefRefPtr<CefFrame> frame,
                                  CefProcessId source_process,
                                  CefRefPtr<CefProcessMessage> message) override;

    QString sessionId() const { return m_sessionId; }

    /// browser 句柄。可能为空。
    CefRefPtr<CefBrowser> browser() const { return m_browser; }

    /// 在主 frame 执行脚本。必须在 CEF UI 线程调用。
    void executeJavaScript(const QString& script);

    /// 关闭 browser。必须在 CEF UI 线程调用。
    void closeBrowser();

private:
    IMPLEMENT_REFCOUNTING(SessionClient);

    SessionManager* m_manager = nullptr;
    QString m_sessionId;
    CefRefPtr<CefBrowser> m_browser;
};

} // namespace browser
} // namespace rustcam
