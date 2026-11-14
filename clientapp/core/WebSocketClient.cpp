#include "WebSocketClient.h"
#include "Logger.h"
#include "ServerApi.h"
#include "ServerConfig.h"

#include <QJsonDocument>
#include <QThread>
#include <QTimer>
#include <QUrl>
#include <QUrlQuery>
#include <QWebSocket>

namespace rustcam {

WsWorker::WsWorker(QObject* parent)
    : QObject(parent)
{
}

void WsWorker::start()
{
    m_socket = new QWebSocket(QString(), QWebSocketProtocol::VersionLatest, this);
    m_retryTimer = new QTimer(this);
    m_retryTimer->setSingleShot(true);

    connect(m_retryTimer, &QTimer::timeout, this, &WsWorker::onRetryTimeout);
    connect(m_socket, &QWebSocket::connected, this, &WsWorker::onConnected);
    connect(m_socket, &QWebSocket::disconnected, this, &WsWorker::onDisconnected);
    connect(m_socket, &QWebSocket::textMessageReceived, this, &WsWorker::onTextMessage);
#if QT_VERSION >= QT_VERSION_CHECK(6, 0, 0)
    connect(m_socket, &QWebSocket::errorOccurred, this, &WsWorker::onSocketError);
#else
    connect(m_socket, QOverload<QAbstractSocket::SocketError>::of(&QWebSocket::error),
            this, &WsWorker::onSocketError);
#endif

    requestTicket();
}

void WsWorker::requestTicket()
{
    if (m_stopped) {
        return;
    }

    // 取票是阻塞式 HTTP 请求，只能在工作线程里做；回调同样运行在本线程。
    ServerApi::instance()->post(
        QStringLiteral("/api/v1/ws/ticket"),
        QJsonObject(),
        [this](bool success, const QJsonObject& response, const QString& error) {
            const QString ticket = response.value("data").toObject().value("ticket").toString();
            if (!success || response.value("code").toInt() != 200 || ticket.isEmpty()) {
                LOG_WARNING("WsWorker", QString("Failed to fetch ws ticket: %1").arg(error));
                scheduleRetry();
                return;
            }
            connectWithTicket(ticket);
        });
}

void WsWorker::connectWithTicket(const QString& ticket)
{
    if (m_stopped || !m_socket) {
        return;
    }

    QUrl url(ServerConfig::instance()->serverUrl());
    url.setScheme(url.scheme() == "https" ? "wss" : "ws");
    url.setPath("/ws");

    QUrlQuery query;
    query.addQueryItem("ticket", ticket);
    url.setQuery(query);

    LOG_INFO("WsWorker", QString("Connecting to %1").arg(url.toString(QUrl::RemoveQuery)));
    m_socket->open(url);
}

void WsWorker::stop()
{
    m_stopped = true;
    if (m_retryTimer) {
        m_retryTimer->stop();
    }
    if (m_socket) {
        m_socket->close();
    }
}

void WsWorker::resume()
{
    m_stopped = false;
    m_delayMs = 1000;
    requestTicket();
}

void WsWorker::onConnected()
{
    m_delayMs = 1000;
    emit connectedChanged(true);
}

void WsWorker::onDisconnected()
{
    emit connectedChanged(false);
    if (m_stopped) {
        return;
    }
    if (m_socket && m_socket->closeCode() == 4401) {
        LOG_WARNING("WsWorker", "Session invalidated by server");
        emit sessionInvalid();
        return;
    }
    scheduleRetry();
}

void WsWorker::onTextMessage(const QString& message)
{
    const QJsonDocument doc = QJsonDocument::fromJson(message.toUtf8());
    if (!doc.isObject()) {
        return;
    }
    const QJsonObject obj = doc.object();
    emit notificationReceived(obj.value("msg_type").toString(),
                              obj.value("data").toObject());
}

void WsWorker::onSocketError()
{
    LOG_WARNING("WsWorker", QString("WebSocket error: %1")
                                .arg(m_socket ? m_socket->errorString() : QString()));
}

void WsWorker::onRetryTimeout()
{
    requestTicket();
}

void WsWorker::scheduleRetry()
{
    if (!m_retryTimer) {
        return;
    }
    m_retryTimer->start(m_delayMs);
    m_delayMs = qMin(m_delayMs * 2, 30000);
}

WebSocketClient::WebSocketClient(QObject* parent)
    : QObject(parent)
{
    m_thread = new QThread(this);
    m_worker = new WsWorker();
    m_worker->moveToThread(m_thread);

    connect(m_thread, &QThread::started, m_worker, &WsWorker::start);
    connect(m_worker, &WsWorker::connectedChanged, this, [this](bool connected) {
        m_connected = connected;
        emit connectedChanged(connected);
    });
    connect(m_worker, &WsWorker::notificationReceived, this,
            &WebSocketClient::notificationReceived);
    connect(m_worker, &WsWorker::sessionInvalid, this, [this]() {
        m_enabled = false;
        emit sessionInvalid();
    });
}

WebSocketClient::~WebSocketClient()
{
    if (m_worker && m_thread->isRunning()) {
        QMetaObject::invokeMethod(m_worker, "stop", Qt::BlockingQueuedConnection);
    }
    if (m_thread) {
        m_thread->quit();
        m_thread->wait(3000);
    }
    delete m_worker;
}

WebSocketClient* WebSocketClient::instance()
{
    static WebSocketClient inst;
    return &inst;
}

void WebSocketClient::setEnabled(bool enabled)
{
    if (m_enabled == enabled) {
        return;
    }
    m_enabled = enabled;

    if (enabled) {
        if (!m_thread->isRunning()) {
            m_thread->start();
        } else {
            QMetaObject::invokeMethod(m_worker, "resume", Qt::QueuedConnection);
        }
    } else {
        QMetaObject::invokeMethod(m_worker, "stop", Qt::QueuedConnection);
        m_connected = false;
    }
}

}
