#pragma once

#include "SessionClient.h"

#include <QHash>
#include <QObject>
#include <QString>

namespace rustcam {
namespace browser {

/// 会话表：sessionId → CefBrowser。
///
/// 一个进程可以同时承载多个会话（多个页面），本类是唯一的拥有者。
class SessionManager : public QObject {
    Q_OBJECT

public:
    explicit SessionManager(QObject* parent = nullptr);

    /// 创建一个会话并在 CEF UI 线程上创建 browser。
    /// @param mode "embedded" 或 "overlay"
    /// @return 失败时返回错误码（browseripc::err::*），成功返回空串
    QString createSession(const QString& sessionId,
                          const QString& url,
                          quint64 hostHandle,
                          const QString& mode);

    /// 关闭并移除一个会话。
    void closeSession(const QString& sessionId);

    /// 在主 frame 执行脚本（会话不存在时忽略）。
    void executeJavaScript(const QString& sessionId, const QString& script);

    /// 调整 overlay 窗口位置与尺寸。
    void setSessionBounds(const QString& sessionId, int x, int y, int w, int h);

    /// 显示 / 隐藏 overlay 窗口。
    void setSessionVisible(const QString& sessionId, bool visible);

    /// 请求把焦点给会话窗口。
    void focusSession(const QString& sessionId);

    /// 启动 parent 存活看门狗：宿主窗口消失时关闭会话，进程没有会话时退出。
    void startParentWatchdog();

    void onSessionCreated(const QString& sessionId, quint64 handle);
    void onSessionReady(const QString& sessionId);
    void onSessionEvent(const QString& sessionId, const QString& payload);
    void onSessionClosed(const QString& sessionId);

signals:
    void sessionCreated(const QString& sessionId, quint64 handle);
    void sessionReady(const QString& sessionId);
    void sessionEvent(const QString& sessionId, const QString& payload);
    void sessionClosed(const QString& sessionId);
    void sessionFailed(const QString& sessionId, const QString& error);

private:
    struct Session {
        CefRefPtr<SessionClient> client;
        QString mode;
        quint64 parentHandle = 0;
    };

    QHash<QString, Session> m_sessions;
};

} // namespace browser
} // namespace rustcam
