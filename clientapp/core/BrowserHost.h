#pragma once

#include <QHash>
#include <QJsonObject>
#include <QObject>
#include <QProcess>
#include <QSet>
#include <QString>
#include <QTimer>

#include <QtGui/qwindowdefs.h>

#include "CoreExport.h"

namespace rustcam {

class BrowserIpcClient;

/// rustcam-browser 子进程的管理者。
///
/// 职责：拉起子进程、完成握手、转发命令与事件、在子进程异常退出时按
/// 指数退避重启并重建会话。
///
/// 本类不认识「地图」——它只认识 session id 与 URL（spec 规则 R2）。
class RUSTCAM_CORE_API BrowserHost : public QObject {
    Q_OBJECT

public:
    struct SessionRequest {
        QString sessionId;
        QString url;
        WId hostHandle = 0;
        QString mode = QStringLiteral("embedded");
    };

    static BrowserHost* instance();

    bool createSession(const SessionRequest& request);
    void closeSession(const QString& sessionId);
    void executeJavaScript(const QString& sessionId, const QString& script);
    void setSessionVisible(const QString& sessionId, bool visible);

    /// overlay 模式专用：下发屏幕坐标矩形。
    void setSessionBounds(const QString& sessionId, int x, int y, int w, int h);

    void focusSession(const QString& sessionId);

    /// rustcam-browser.exe 的绝对路径。
    QString browserExecutablePath() const;

signals:
    void sessionCreated(const QString& sessionId, quint64 handle);
    void sessionReady(const QString& sessionId);
    void sessionEvent(const QString& sessionId, const QString& payload);
    void sessionClosed(const QString& sessionId);
    void sessionFailed(const QString& sessionId, const QString& error);
    void browserProcessStarted();
    void browserProcessCrashed();

private:
    explicit BrowserHost(QObject* parent = nullptr);

    bool ensureProcess();
    bool startProcess();
    void handleMessage(const QJsonObject& message);
    void scheduleRestart();
    void restartNow();
    void sendCreateSession(const SessionRequest& request);
    void sendPendingSessions();
    void shutdownBrowser();
    QString pipeName() const;

    QProcess* m_process = nullptr;
    BrowserIpcClient* m_client = nullptr;
    QTimer* m_restartTimer = nullptr;
    int m_restartAttempt = 0;
    bool m_stopping = false;
    bool m_handshakeOk = false;

    /// 每个会话的期望状态：子进程重启后据此重建（spec §7.1）。
    QHash<QString, SessionRequest> m_pendingRequests;

    /// 已经在当前连接上发过 session.create 的会话（避免重复下发）。
    QSet<QString> m_sentRequests;
};

} // namespace rustcam
