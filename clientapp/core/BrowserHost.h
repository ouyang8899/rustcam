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
/// **启动慢不等于失败**：浏览器进程要初始化 CEF 才 listen()，冷启动可能
/// 要几十秒。这段时间里只等待并上报进度，绝不杀进程重启——杀掉一个正在
/// 启动的浏览器既拿不到会话，又会留下持有 Chromium profile 单例锁的孤儿
/// 进程，让之后每一次启动都直接失败。
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

    /// 测试用：改写浏览器可执行文件路径。
    void setExecutablePathOverride(const QString& path);

    /// 测试用：附加到浏览器进程命令行末尾的参数。
    void setExtraArguments(const QStringList& args);

    /// 浏览器进程是否已经握手成功。
    bool isHandshakeOk() const { return m_handshakeOk; }

signals:
    void sessionCreated(const QString& sessionId, quint64 handle);
    void sessionReady(const QString& sessionId);
    void sessionEvent(const QString& sessionId, const QString& payload);
    void sessionClosed(const QString& sessionId);
    void sessionFailed(const QString& sessionId, const QString& error);
    void browserProcessStarted();

    /// 浏览器进程仍在启动，已等待 elapsedMs 毫秒。
    void browserProcessWaiting(int elapsedMs);

    /// 浏览器进程启动超时（进程还活着，但一直没有握手）。
    void browserProcessTimeout();

    /// 浏览器进程反复崩溃，重启次数用尽。
    void browserProcessCrashed();

private:
    explicit BrowserHost(QObject* parent = nullptr);

    bool ensureProcess();
    bool startProcess();
    void handleMessage(const QJsonObject& message);
    void scheduleRestart(const char* reason);
    void restartNow();
    void sendCreateSession(const SessionRequest& request);
    void sendPendingSessions();
    void shutdownBrowser();
    void killProcess();
    void removeBrowserProfile(qint64 pid);
    QStringList browserArguments() const;
    QString browserCacheRoot() const;
    QString pipeName() const;

    QProcess* m_process = nullptr;
    BrowserIpcClient* m_client = nullptr;
    QTimer* m_restartTimer = nullptr;
    QTimer* m_startupTimer = nullptr;
    int m_restartAttempt = 0;
    bool m_stopping = false;
    bool m_handshakeOk = false;

    /// 主动断开 IPC（重启 / 退出）时置位：随后的 disconnected 不算故障。
    /// 新进程握手成功后清零。
    bool m_intentionalDisconnect = false;

    QString m_executablePathOverride;
    QStringList m_extraArguments;

    /// 每个会话的期望状态：子进程重启后据此重建（spec §7.1）。
    QHash<QString, SessionRequest> m_pendingRequests;

    /// 已经在当前连接上发过 session.create 的会话（避免重复下发）。
    QSet<QString> m_sentRequests;
};

} // namespace rustcam
