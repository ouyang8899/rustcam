#pragma once

#include <QByteArray>
#include <QElapsedTimer>
#include <QJsonObject>
#include <QObject>
#include <QString>
#include <QTimer>

class QLocalSocket;

namespace rustcam {

/// clientapp 侧的 IPC 客户端：连接 rustcam-browser，按协议收发 JSON 帧。
///
/// 握手是**异步**的：connectToServer() 立即返回，连上后自动发 hello，
/// 收到 hello.ok 时发出 connected() 信号。
/// 不用同步等待是为了不冻结 UI——首次打开地图时浏览器进程可能还没起来。
///
/// **连接会一直重试**：浏览器进程要先 CefInitialize 才 listen()，管道在
/// 这之前并不存在；Qt 的 QLocalSocket 在 Windows 上不会自动重试，所以这里
/// 用定时器反复尝试。等待期间只发 waitProgress()，**不算失败**——把「启动
/// 慢」当成「进程崩了」去杀进程重启，只会让浏览器永远起不来。
class BrowserIpcClient : public QObject {
    Q_OBJECT

public:
    explicit BrowserIpcClient(QObject* parent = nullptr);
    ~BrowserIpcClient() override;

    /// 发起连接（异步，持续重试直到连上）。连接成功后自动发送 hello，
    /// 收到 hello.ok 时发出 connected() 信号。
    void connectToServer(const QString& pipeName);

    /// socket 层面的连接状态。
    bool isConnected() const;

    /// 握手是否已经完成（收到 hello.ok）。
    bool isReady() const { return m_handshakeOk; }

    /// 已经在等待连接（进程已启动但还没 listen）。
    bool isWaiting() const { return m_waiting; }

    void send(const QJsonObject& message);

    void disconnectFromServer();

signals:
    void messageReceived(const QJsonObject& message);
    void connected();
    void disconnected();

    /// 仍在等待浏览器进程就绪，已等待 elapsedMs 毫秒。
    void waitProgress(int elapsedMs);

private slots:
    void onSocketConnected();
    void onReadyRead();
    void onDisconnected();
    void onConnectRetry();

private:
    void handleMessage(const QJsonObject& message);
    void flushRxBuffer();

    QLocalSocket* m_socket = nullptr;
    QTimer* m_connectTimer = nullptr;
    QElapsedTimer m_connectDeadline;
    QString m_pipeName;
    QByteArray m_rxBuffer;
    bool m_handshakeDone = false;
    bool m_handshakeOk = false;
    bool m_waiting = false;

    /// 本次连接是否曾经建立过：建立失败（管道还不存在）不算「连上又断了」。
    bool m_connectedOnce = false;

    int m_lastReportedWaitMs = 0;
};

} // namespace rustcam
