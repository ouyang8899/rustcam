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
/// **连接会重试**：浏览器进程要先 CefInitialize（约几百毫秒）才 listen()，
/// 管道在这之前并不存在；Qt 的 QLocalSocket 在 Windows 上不会自动重试，
/// 所以这里用定时器反复尝试，直到连上或超时。
class BrowserIpcClient : public QObject {
    Q_OBJECT

public:
    explicit BrowserIpcClient(QObject* parent = nullptr);
    ~BrowserIpcClient() override;

    /// 发起连接（异步，带重试）。连接成功后自动发送 hello，
    /// 收到 hello.ok 时发出 connected() 信号。
    /// @param timeoutMs 超过该时间仍未连上则放弃并发出 disconnected()
    void connectToServer(const QString& pipeName, int timeoutMs = 10000);

    /// socket 层面的连接状态。
    bool isConnected() const;

    /// 握手是否已经完成（收到 hello.ok）。
    bool isReady() const { return m_handshakeOk; }

    void send(const QJsonObject& message);

    void disconnectFromServer();

signals:
    void messageReceived(const QJsonObject& message);
    void connected();
    void disconnected();

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
    bool m_connectTimedOut = false;
};

} // namespace rustcam
