#include "IpcServer.h"

#include "common/BrowserIpcProtocol.h"

#include <QCoreApplication>
#include <QJsonObject>
#include <QLocalServer>
#include <QLocalSocket>
#include <QTextStream>
#include <QTimer>
#include <cstdio>

namespace rustcam {
namespace browser {

using namespace rustcam::browseripc;

IpcServer::IpcServer(SessionManager* manager, QObject* parent)
    : QObject(parent)
    , m_manager(manager)
{
    connect(m_manager, &SessionManager::sessionCreated,
            this, &IpcServer::onSessionCreated);
    connect(m_manager, &SessionManager::sessionReady,
            this, &IpcServer::onSessionReady);
    connect(m_manager, &SessionManager::sessionEvent,
            this, &IpcServer::onSessionEvent);
    connect(m_manager, &SessionManager::sessionClosed,
            this, &IpcServer::onSessionClosed);
    connect(m_manager, &SessionManager::sessionFailed,
            this, &IpcServer::onSessionFailed);
}

bool IpcServer::listen(const QString& pipeName, QString* error)
{
    m_server = new QLocalServer(this);
    if (!m_server->listen(pipeName)) {
        if (error) {
            *error = m_server->errorString();
        }
        return false;
    }
    connect(m_server, &QLocalServer::newConnection,
            this, &IpcServer::onNewConnection);
    return true;
}

void IpcServer::onNewConnection()
{
    QLocalSocket* incoming = m_server->nextPendingConnection();
    if (!incoming) {
        return;
    }
    // 接管**最新**的连接：clientapp 在握手前可能重试过若干次，
    // 那些旧连接已经被它 abort。若这里只保留第一个连接，clientapp 手上
    // 的那个反而会被当成多余连接丢掉，表现为「连上了但收不到任何消息」。
    if (m_client) {
        m_client->disconnectFromServer();
        m_client->deleteLater();
        m_client = nullptr;
    }
    m_client = incoming;
    m_rxBuffer.clear();
    connect(m_client, &QLocalSocket::readyRead, this, &IpcServer::onReadyRead);
    connect(m_client, &QLocalSocket::disconnected, this, &IpcServer::onDisconnected);

    // 竞态：客户端可能在连接建立后立刻写入（hello），此时数据已经在管道
    // 缓冲区里，而 readyRead 信号在 connect() 之前就已经发过了——信号会丢。
    // 因此连接完信号后主动 drain 一次。
    onReadyRead();
}

void IpcServer::onReadyRead()
{
    if (!m_client) {
        return;
    }
    m_rxBuffer += m_client->readAll();

    while (m_rxBuffer.size() >= 4) {
        const quint32 length = (static_cast<quint8>(m_rxBuffer[0]) << 24)
                            | (static_cast<quint8>(m_rxBuffer[1]) << 16)
                            | (static_cast<quint8>(m_rxBuffer[2]) << 8)
                            | static_cast<quint8>(m_rxBuffer[3]);
        if (length == 0 || length > static_cast<quint32>(kMaxFrameSize)) {
            // 帧长非法：无法恢复，直接断开。
            m_client->abort();
            return;
        }
        if (m_rxBuffer.size() < static_cast<int>(length) + 4) {
            return; // 等更多数据
        }
        const QByteArray frame = m_rxBuffer.left(static_cast<int>(length) + 4);
        m_rxBuffer.remove(0, static_cast<int>(length) + 4);

        QJsonObject message;
        if (!decodeFrame(frame, &message)) {
            reportError(m_client, err::kBadJson);
            continue;
        }
        handleMessage(m_client, message);
    }
}

void IpcServer::onDisconnected()
{
    if (m_client) {
        m_client->deleteLater();
        m_client = nullptr;
    }
    m_rxBuffer.clear();
}

void IpcServer::sendMessage(QLocalSocket* socket, const QJsonObject& message)
{
    if (!socket || socket->state() != QLocalSocket::ConnectedState) {
        return;
    }
    socket->write(encodeFrame(message));
    socket->flush();
}

void IpcServer::reportError(QLocalSocket* socket, const char* code)
{
    QJsonObject err;
    err.insert(QStringLiteral("type"), QString::fromLatin1(msg::kError));
    err.insert(QStringLiteral("code"), QString::fromLatin1(code));
    sendMessage(socket, err);
}

void IpcServer::handleMessage(QLocalSocket* socket, const QJsonObject& message)
{
    const QString type = message.value(QStringLiteral("type")).toString();

    if (type == QLatin1String(msg::kHello)) {
        QJsonObject reply;
        reply.insert(QStringLiteral("type"), QString::fromLatin1(msg::kHelloOk));
        reply.insert(QStringLiteral("protocol"), kProtocolVersion);
        reply.insert(QStringLiteral("pid"), QCoreApplication::applicationPid());
        sendMessage(socket, reply);
        return;
    }

    if (type == QLatin1String(msg::kSessionCreate)) {
        handleCreateSession(socket, message);
        return;
    }

    if (type == QLatin1String(msg::kSessionExec)) {
        const QString sessionId = message.value(QStringLiteral("session")).toString();
        const QString script = message.value(QStringLiteral("script")).toString();
        m_manager->executeJavaScript(sessionId, script);
        return;
    }

    if (type == QLatin1String(msg::kSessionClose)) {
        const QString sessionId = message.value(QStringLiteral("session")).toString();
        m_manager->closeSession(sessionId);
        return;
    }

    if (type == QLatin1String(msg::kSessionSetBounds)) {
        const QString sessionId = message.value(QStringLiteral("session")).toString();
        const QJsonObject bounds = message.value(QStringLiteral("bounds")).toObject();
        m_manager->setSessionBounds(sessionId,
                                    bounds.value(QStringLiteral("x")).toInt(),
                                    bounds.value(QStringLiteral("y")).toInt(),
                                    bounds.value(QStringLiteral("width")).toInt(),
                                    bounds.value(QStringLiteral("height")).toInt());
        return;
    }

    if (type == QLatin1String(msg::kSessionSetVisible)) {
        const QString sessionId = message.value(QStringLiteral("session")).toString();
        const bool visible = message.value(QStringLiteral("visible")).toBool();
        m_manager->setSessionVisible(sessionId, visible);
        return;
    }

    if (type == QLatin1String(msg::kSessionFocus)) {
        const QString sessionId = message.value(QStringLiteral("session")).toString();
        m_manager->focusSession(sessionId);
        return;
    }

    if (type == QLatin1String(msg::kShutdown)) {
        emit shutdownRequested();
        return;
    }

    reportError(socket, err::kUnknownType);
}

void IpcServer::handleCreateSession(QLocalSocket* socket, const QJsonObject& message)
{
    const QString sessionId = message.value(QStringLiteral("session")).toString();
    const QString url = message.value(QStringLiteral("url")).toString();
    const QString mode = message.value(QStringLiteral("mode")).toString();
    const QJsonObject host = message.value(QStringLiteral("host")).toObject();
    const QString hostKind = host.value(QStringLiteral("kind")).toString();
    const QString hostHandleText = host.value(QStringLiteral("handle")).toString();

    quint64 hostHandle = 0;
    if (!hostHandleText.isEmpty() && !decodeHandle(hostHandleText, &hostHandle)) {
        reportError(socket, err::kBadRequest);
        return;
    }

    // host.kind 与 mode 必须自洽（spec §4.2）。
    if (mode == QLatin1String(mode::kEmbedded)
        && hostKind != QLatin1String(hostkind::kWin32)
        && hostKind != QLatin1String(hostkind::kX11)) {
        reportError(socket, err::kBadRequest);
        return;
    }

    const QString error = m_manager->createSession(sessionId, url, hostHandle, mode);
    if (!error.isEmpty()) {
        QJsonObject failed;
        failed.insert(QStringLiteral("type"), QString::fromLatin1(msg::kSessionFailed));
        failed.insert(QStringLiteral("session"), sessionId);
        failed.insert(QStringLiteral("error"), error);
        sendMessage(socket, failed);
        return;
    }
}

void IpcServer::onSessionCreated(const QString& sessionId, quint64 handle)
{
    if (!m_client) {
        return;
    }
    QJsonObject message;
    message.insert(QStringLiteral("type"), QString::fromLatin1(msg::kSessionCreated));
    message.insert(QStringLiteral("session"), sessionId);
    message.insert(QStringLiteral("hwnd"), encodeHandle(handle));
    sendMessage(m_client, message);
}

void IpcServer::onSessionReady(const QString& sessionId)
{
    if (!m_client) {
        return;
    }
    QJsonObject message;
    message.insert(QStringLiteral("type"), QString::fromLatin1(msg::kSessionReady));
    message.insert(QStringLiteral("session"), sessionId);
    sendMessage(m_client, message);
}

void IpcServer::onSessionEvent(const QString& sessionId, const QString& payload)
{
    if (!m_client) {
        return;
    }
    QJsonObject message;
    message.insert(QStringLiteral("type"), QString::fromLatin1(msg::kSessionEvent));
    message.insert(QStringLiteral("session"), sessionId);
    message.insert(QStringLiteral("payload"), payload);
    sendMessage(m_client, message);
}

void IpcServer::onSessionClosed(const QString& sessionId)
{
    if (!m_client) {
        return;
    }
    QJsonObject message;
    message.insert(QStringLiteral("type"), QString::fromLatin1(msg::kSessionClosed));
    message.insert(QStringLiteral("session"), sessionId);
    sendMessage(m_client, message);
}

void IpcServer::onSessionFailed(const QString& sessionId, const QString& error)
{
    if (!m_client) {
        return;
    }
    QJsonObject message;
    message.insert(QStringLiteral("type"), QString::fromLatin1(msg::kSessionFailed));
    message.insert(QStringLiteral("session"), sessionId);
    message.insert(QStringLiteral("error"), error);
    sendMessage(m_client, message);
}

} // namespace browser
} // namespace rustcam
