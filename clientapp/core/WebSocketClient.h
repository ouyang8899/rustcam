#pragma once

#include <QObject>
#include <QJsonObject>
#include <QString>
#include "CoreExport.h"

class QThread;
class QWebSocket;
class QTimer;

namespace rustcam {

/**
 * WebSocket 工作线程：取票、连接、收发消息全部在这里完成。
 *
 * 取票是一次阻塞式 HTTP 请求，绝不能放在 GUI 线程上执行，否则整个界面会卡死。
 */
class RUSTCAM_CORE_API WsWorker : public QObject {
    Q_OBJECT
public:
    explicit WsWorker(QObject* parent = nullptr);

public slots:
    void start();
    void requestTicket();
    void stop();
    void resume();

signals:
    void connectedChanged(bool connected);
    void notificationReceived(const QString& msgType, const QJsonObject& data);
    void sessionInvalid();

private slots:
    void onConnected();
    void onDisconnected();
    void onTextMessage(const QString& message);
    void onSocketError();
    void onRetryTimeout();

private:
    void connectWithTicket(const QString& ticket);
    void scheduleRetry();

    QWebSocket* m_socket = nullptr;
    QTimer* m_retryTimer = nullptr;
    int m_delayMs = 1000;
    bool m_stopped = false;
};

class RUSTCAM_CORE_API WebSocketClient : public QObject {
    Q_OBJECT
public:
    static WebSocketClient* instance();

    void setEnabled(bool enabled);
    bool isConnected() const { return m_connected; }

signals:
    void connectedChanged(bool connected);
    void notificationReceived(const QString& msgType, const QJsonObject& data);
    void sessionInvalid();

private:
    explicit WebSocketClient(QObject* parent = nullptr);
    ~WebSocketClient();
    Q_DISABLE_COPY(WebSocketClient)

    QThread* m_thread = nullptr;
    WsWorker* m_worker = nullptr;
    bool m_enabled = false;
    bool m_connected = false;
};

}
