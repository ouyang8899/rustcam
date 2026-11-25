#include "BrowserIpcClient.h"

#include "common/BrowserIpcProtocol.h"
#include "common/Logger.h"

#include <QCoreApplication>
#include <QLocalSocket>

namespace rustcam {

using namespace rustcam::browseripc;

namespace {

/// 连接重试间隔（毫秒）。
const int kConnectRetryIntervalMs = 100;

/// waitProgress() 的上报间隔（毫秒）。
const int kWaitReportIntervalMs = 1000;

} // namespace

BrowserIpcClient::BrowserIpcClient(QObject* parent)
    : QObject(parent)
    , m_socket(new QLocalSocket(this))
    , m_connectTimer(new QTimer(this))
{
    m_connectTimer->setInterval(kConnectRetryIntervalMs);

    connect(m_socket, &QLocalSocket::connected,
            this, &BrowserIpcClient::onSocketConnected);
    connect(m_socket, &QLocalSocket::readyRead, this, &BrowserIpcClient::onReadyRead);
    connect(m_socket, &QLocalSocket::disconnected, this, &BrowserIpcClient::onDisconnected);
    connect(m_connectTimer, &QTimer::timeout, this, &BrowserIpcClient::onConnectRetry);
}

BrowserIpcClient::~BrowserIpcClient()
{
    disconnectFromServer();
}

void BrowserIpcClient::connectToServer(const QString& pipeName)
{
    m_pipeName = pipeName;
    m_handshakeDone = false;
    m_handshakeOk = false;
    m_waiting = true;
    m_connectedOnce = false;
    m_lastReportedWaitMs = 0;
    m_rxBuffer.clear();

    if (m_socket->state() != QLocalSocket::UnconnectedState) {
        m_socket->abort();
    }

    // 管道可能还不存在（浏览器进程还在初始化 CEF），定时重试。
    m_connectDeadline.start();
    m_connectTimer->start();
    onConnectRetry();
}

void BrowserIpcClient::onConnectRetry()
{
    if (!m_waiting) {
        return;
    }
    const int elapsed = static_cast<int>(m_connectDeadline.elapsed());
    if (elapsed - m_lastReportedWaitMs >= kWaitReportIntervalMs) {
        m_lastReportedWaitMs = elapsed;
        LOG_INFO("BrowserIpcClient",
                 QString("waiting for browser process, %1ms elapsed").arg(elapsed));
        emit waitProgress(elapsed);
    }
    if (m_socket->state() != QLocalSocket::UnconnectedState) {
        // 连接尝试正在进行中，不要重复发起——中途 abort 会让服务端以为
        // 客户端已连上而把后续连接当成多余连接断掉。
        return;
    }
    m_socket->connectToServer(m_pipeName);
}

bool BrowserIpcClient::isConnected() const
{
    return m_socket->state() == QLocalSocket::ConnectedState;
}

void BrowserIpcClient::send(const QJsonObject& message)
{
    if (!isConnected()) {
        return;
    }
    m_socket->write(encodeFrame(message));
    m_socket->flush();
}

void BrowserIpcClient::disconnectFromServer()
{
    m_connectTimer->stop();
    m_waiting = false;
    if (m_socket->state() != QLocalSocket::UnconnectedState) {
        m_socket->disconnectFromServer();
    }
}

void BrowserIpcClient::onSocketConnected()
{
    m_connectedOnce = true;
    // 重试定时器继续跑：管道连上不等于握手完成，hello.ok 之前都算「还在等」。
    QJsonObject hello;
    hello.insert(QStringLiteral("type"), QString::fromLatin1(msg::kHello));
    hello.insert(QStringLiteral("protocol"), kProtocolVersion);
    hello.insert(QStringLiteral("clientPid"), QCoreApplication::applicationPid());
    m_socket->write(encodeFrame(hello));
    m_socket->flush();
}

void BrowserIpcClient::onReadyRead()
{
    m_rxBuffer += m_socket->readAll();
    flushRxBuffer();
}

void BrowserIpcClient::flushRxBuffer()
{
    while (m_rxBuffer.size() >= 4) {
        const quint32 length = (static_cast<quint8>(m_rxBuffer[0]) << 24)
                            | (static_cast<quint8>(m_rxBuffer[1]) << 16)
                            | (static_cast<quint8>(m_rxBuffer[2]) << 8)
                            | static_cast<quint8>(m_rxBuffer[3]);
        if (length == 0 || length > static_cast<quint32>(kMaxFrameSize)) {
            // 帧长非法：无法恢复，直接断开。
            m_socket->abort();
            return;
        }
        if (m_rxBuffer.size() < static_cast<int>(length) + 4) {
            return;
        }
        const QByteArray frame = m_rxBuffer.left(static_cast<int>(length) + 4);
        m_rxBuffer.remove(0, static_cast<int>(length) + 4);

        QJsonObject message;
        if (!decodeFrame(frame, &message)) {
            continue;
        }
        handleMessage(message);
    }
}

void BrowserIpcClient::handleMessage(const QJsonObject& message)
{
    const QString type = message.value(QStringLiteral("type")).toString();

    if (type == QLatin1String(msg::kHelloOk)) {
        m_handshakeDone = true;
        m_handshakeOk = message.value(QStringLiteral("protocol")).toInt() == kProtocolVersion;
        m_waiting = false;
        emit connected();
        return;
    }

    emit messageReceived(message);
}

void BrowserIpcClient::onDisconnected()
{
    m_connectTimer->stop();
    if (!m_connectedOnce) {
        // 管道还不存在（浏览器进程还在初始化 CEF）：这是「还没连上」，
        // 不是「连上又断了」，不算故障，继续等。
        m_connectTimer->start();
        return;
    }
    m_handshakeDone = true;
    m_handshakeOk = false;
    m_waiting = false;
    emit disconnected();
}

} // namespace rustcam
