#include "SessionManager.h"
#include "OverlayWindow.h"
#include "SessionClient.h"

#include "include/base/cef_bind.h"
#include "include/base/cef_callback.h"
#include "include/cef_browser.h"
#include "include/cef_task.h"
#include "include/wrapper/cef_closure_task.h"

#include "common/BrowserIpcProtocol.h"

#include <QCoreApplication>
#include <QTimer>
#include <windows.h>

#include <cstdio>

#ifdef Q_OS_WIN
#include <windows.h>
#endif

namespace rustcam {
namespace browser {

using namespace rustcam::browseripc;

SessionManager::SessionManager(QObject* parent)
    : QObject(parent)
{
}

QString SessionManager::createSession(const QString& sessionId,
                                      const QString& url,
                                      quint64 hostHandle,
                                      const QString& mode)
{
    if (m_sessions.contains(sessionId)) {
        return QString::fromLatin1(err::kBadRequest);
    }
    if (mode != QLatin1String(mode::kEmbedded) && mode != QLatin1String(mode::kOverlay)) {
        return QString::fromLatin1(err::kBadRequest);
    }
    if (mode == QLatin1String(mode::kEmbedded) && hostHandle == 0) {
        return QString::fromLatin1(err::kBadRequest);
    }
    if (url.isEmpty()) {
        return QString::fromLatin1(err::kBadRequest);
    }

    Session session;
    session.client = new SessionClient(this, sessionId);
    session.mode = mode;
    session.parentHandle = hostHandle;
    m_sessions.insert(sessionId, session);

    std::fprintf(stderr, "[SessionManager] createSession id=%s mode=%s hostHandle=%llu url=%s\n",
                 sessionId.toUtf8().constData(),
                 mode.toUtf8().constData(),
                 static_cast<unsigned long long>(hostHandle),
                 url.toUtf8().constData());
    std::fflush(stderr);

    // browser 的创建必须在 CEF UI 线程。
    CefRefPtr<SessionClient> client = session.client;
    CefPostTask(TID_UI, base::BindOnce(
        [](CefRefPtr<SessionClient> client, QString url, quint64 hostHandle,
           QString mode) {
            CefWindowInfo windowInfo;
            if (mode == QLatin1String(mode::kEmbedded)) {
                // 跨进程 parent：Windows 允许子窗口的 parent 属于其他进程，
                // 窗口消息仍送到创建它的线程（spec §5.2，spike S1 已验证）。
                windowInfo.SetAsChild(fromPortableHandle(hostHandle),
                                      CefRect(0, 0, 800, 600));
                // CEF 156 默认 Chrome runtime，它不会创建子窗口：SetAsChild
                // 的 WS_CHILD/parent 会被忽略，浏览器变成屏幕上的顶级窗口。
                // 子窗口必须显式用 Alloy style（cef_types_runtime.h）。
                windowInfo.runtime_style = CEF_RUNTIME_STYLE_ALLOY;
            } else {
                // overlay 模式：顶级窗口（CEF 156 的 SetAsPopup 签名是
                // (parent, windowName)，不再接受 bounds）。
                windowInfo.SetAsPopup(static_cast<CefWindowHandle>(0),
                                      CefString("rustcam-browser"));
            }

            CefBrowserSettings browserSettings;
            std::fprintf(stderr, "[SessionManager] CreateBrowser mode=%s hostHandle=%llu\n",
                         mode.toUtf8().constData(),
                         static_cast<unsigned long long>(hostHandle));
            std::fflush(stderr);
            CefBrowserHost::CreateBrowser(windowInfo, client,
                                         CefString(url.toStdString()),
                                         browserSettings, nullptr, nullptr);
        },
        client, url, hostHandle, mode));

    return QString();
}

void SessionManager::closeSession(const QString& sessionId)
{
    auto it = m_sessions.find(sessionId);
    if (it == m_sessions.end()) {
        return;
    }
    CefRefPtr<SessionClient> client = it->client;
    if (!client) {
        return;
    }
    CefPostTask(TID_UI, base::BindOnce(
        [](CefRefPtr<SessionClient> client) {
            if (client) {
                client->closeBrowser();
            }
        },
        client));
}

void SessionManager::executeJavaScript(const QString& sessionId, const QString& script)
{
    auto it = m_sessions.find(sessionId);
    if (it == m_sessions.end() || !it->client) {
        return;
    }
    CefRefPtr<SessionClient> client = it->client;
    CefPostTask(TID_UI, base::BindOnce(
        [](CefRefPtr<SessionClient> client, QString script) {
            if (client) {
                client->executeJavaScript(script);
            }
        },
        client, script));
}

void SessionManager::setSessionBounds(const QString& sessionId, int x, int y, int w, int h)
{
    auto it = m_sessions.find(sessionId);
    if (it == m_sessions.end() || !it->client) {
        return;
    }
    CefRefPtr<CefBrowser> browser = it->client->browser();
    if (!browser) {
        return;
    }
    CefPostTask(TID_UI, base::BindOnce(
        [](CefRefPtr<CefBrowser> browser, int x, int y, int w, int h) {
            if (!browser) {
                return;
            }
            overlayMoveResize(browser->GetHost()->GetWindowHandle(), x, y, w, h);
            browser->GetHost()->WasResized();
        },
        browser, x, y, w, h));
}

void SessionManager::setSessionVisible(const QString& sessionId, bool visible)
{
    auto it = m_sessions.find(sessionId);
    if (it == m_sessions.end() || !it->client) {
        return;
    }
    CefRefPtr<CefBrowser> browser = it->client->browser();
    if (!browser) {
        return;
    }
    CefPostTask(TID_UI, base::BindOnce(
        [](CefRefPtr<CefBrowser> browser, bool visible) {
            if (!browser) {
                return;
            }
            overlaySetVisible(browser->GetHost()->GetWindowHandle(), visible);
        },
        browser, visible));
}

void SessionManager::focusSession(const QString& sessionId)
{
    auto it = m_sessions.find(sessionId);
    if (it == m_sessions.end() || !it->client) {
        return;
    }
    CefRefPtr<CefBrowser> browser = it->client->browser();
    if (!browser) {
        return;
    }
    CefPostTask(TID_UI, base::BindOnce(
        [](CefRefPtr<CefBrowser> browser) {
            if (!browser) {
                return;
            }
            overlayFocus(browser->GetHost()->GetWindowHandle());
        },
        browser));
}

void SessionManager::startParentWatchdog()
{
    QTimer* timer = new QTimer(this);
    timer->setInterval(1000);
    connect(timer, &QTimer::timeout, this, [this]() {
        bool anyAlive = false;
        const QStringList ids = m_sessions.keys();
        for (const QString& id : ids) {
            auto it = m_sessions.find(id);
            if (it == m_sessions.end()) {
                continue;
            }
            if (it->parentHandle == 0) {
                // overlay 模式没有宿主窗口，由 clientapp 显式关闭。
                anyAlive = true;
                continue;
            }
#ifdef Q_OS_WIN
            if (IsWindow(reinterpret_cast<HWND>(it->parentHandle))) {
                anyAlive = true;
            } else {
                closeSession(id);
            }
#else
            anyAlive = true;
#endif
        }
        if (!anyAlive && !m_sessions.isEmpty()) {
            // 所有会话的宿主窗口都消失了，进程没有存在价值。
            QCoreApplication::quit();
        }
    });
    timer->start();
}

void SessionManager::onSessionCreated(const QString& sessionId, quint64 handle)
{
    emit sessionCreated(sessionId, handle);
}

void SessionManager::onSessionReady(const QString& sessionId)
{
    emit sessionReady(sessionId);
}

void SessionManager::onSessionEvent(const QString& sessionId, const QString& payload)
{
    emit sessionEvent(sessionId, payload);
}

void SessionManager::onSessionClosed(const QString& sessionId)
{
    m_sessions.remove(sessionId);
    emit sessionClosed(sessionId);
}

} // namespace browser
} // namespace rustcam
