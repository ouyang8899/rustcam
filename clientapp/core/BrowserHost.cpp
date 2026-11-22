#include "BrowserHost.h"

#include "BrowserIpcClient.h"

#include "common/BrowserIpcProtocol.h"
#include "platform/BrowserWindowHost.h"

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QJsonObject>
#include <QTextStream>

namespace rustcam {

using namespace rustcam::browseripc;

namespace {

/// 退避序列（毫秒）。
const int kRestartBackoffMs[] = { 1000, 2000, 4000, 8000, 16000, 30000 };
const int kRestartBackoffCount = 6;
const int kMaxRestartAttempts = 5;

} // namespace

BrowserHost* BrowserHost::instance()
{
    static BrowserHost host;
    return &host;
}

BrowserHost::BrowserHost(QObject* parent)
    : QObject(parent)
    , m_client(new BrowserIpcClient(this))
    , m_restartTimer(new QTimer(this))
{
    m_restartTimer->setSingleShot(true);

    connect(m_client, &BrowserIpcClient::messageReceived,
            this, &BrowserHost::handleMessage);
    connect(m_client, &BrowserIpcClient::connected, this, [this]() {
        m_handshakeOk = true;
        m_restartAttempt = 0;
        emit browserProcessStarted();
        // 握手完成后才能下发 session.create。
        sendPendingSessions();
    });
    connect(m_client, &BrowserIpcClient::disconnected, this, [this]() {
        m_handshakeOk = false;
        m_sentRequests.clear();
        if (!m_stopping) {
            scheduleRestart();
        }
    });
    connect(m_restartTimer, &QTimer::timeout, this, &BrowserHost::restartNow);
    connect(qApp, &QCoreApplication::aboutToQuit,
            this, &BrowserHost::shutdownBrowser);
}

QString BrowserHost::pipeName() const
{
    return QStringLiteral("rustcam-browser-%1").arg(QCoreApplication::applicationPid());
}

QString BrowserHost::browserExecutablePath() const
{
    const QString exeDir = QCoreApplication::applicationDirPath();
    const QString exeName = QStringLiteral("rustcam-browser.exe");

    // 多配置生成器（Visual Studio）会把可执行文件放进 <out>/<Config>/，
    // 而浏览器进程又被放在 <out>/rustcam-browser/<Config>/ 下。
    // 两种布局都试一遍。
    const QString configDir = QDir(exeDir).dirName();
    const QStringList candidates = {
        QDir(exeDir).absoluteFilePath(QStringLiteral("rustcam-browser/") + exeName),
        QDir(exeDir).absoluteFilePath(
            QStringLiteral("../rustcam-browser/%1/%2").arg(configDir, exeName)),
        QDir(exeDir).absoluteFilePath(QStringLiteral("../rustcam-browser/") + exeName),
    };

    for (const QString& candidate : candidates) {
        if (QFileInfo::exists(candidate)) {
            return QDir(candidate).absolutePath();
        }
    }
    return candidates.first();
}

bool BrowserHost::createSession(const SessionRequest& request)
{
    m_pendingRequests.insert(request.sessionId, request);

    if (!ensureProcess()) {
        return false;
    }
    if (m_handshakeOk) {
        sendCreateSession(request);
        m_sentRequests.insert(request.sessionId);
    }
    return true;
}

void BrowserHost::closeSession(const QString& sessionId)
{
    m_pendingRequests.remove(sessionId);
    m_sentRequests.remove(sessionId);
    QJsonObject message;
    message.insert(QStringLiteral("type"), QString::fromLatin1(msg::kSessionClose));
    message.insert(QStringLiteral("session"), sessionId);
    m_client->send(message);
}

void BrowserHost::executeJavaScript(const QString& sessionId, const QString& script)
{
    QJsonObject message;
    message.insert(QStringLiteral("type"), QString::fromLatin1(msg::kSessionExec));
    message.insert(QStringLiteral("session"), sessionId);
    message.insert(QStringLiteral("script"), script);
    m_client->send(message);
}

void BrowserHost::setSessionVisible(const QString& sessionId, bool visible)
{
    QJsonObject message;
    message.insert(QStringLiteral("type"), QString::fromLatin1(msg::kSessionSetVisible));
    message.insert(QStringLiteral("session"), sessionId);
    message.insert(QStringLiteral("visible"), visible);
    m_client->send(message);
}

void BrowserHost::setSessionBounds(const QString& sessionId, int x, int y, int w, int h)
{
    QJsonObject bounds;
    bounds.insert(QStringLiteral("x"), x);
    bounds.insert(QStringLiteral("y"), y);
    bounds.insert(QStringLiteral("width"), w);
    bounds.insert(QStringLiteral("height"), h);

    QJsonObject message;
    message.insert(QStringLiteral("type"), QString::fromLatin1(msg::kSessionSetBounds));
    message.insert(QStringLiteral("session"), sessionId);
    message.insert(QStringLiteral("bounds"), bounds);
    m_client->send(message);
}

void BrowserHost::focusSession(const QString& sessionId)
{
    QJsonObject message;
    message.insert(QStringLiteral("type"), QString::fromLatin1(msg::kSessionFocus));
    message.insert(QStringLiteral("session"), sessionId);
    m_client->send(message);
}

bool BrowserHost::ensureProcess()
{
    if (m_process && m_process->state() == QProcess::Running) {
        return true;
    }
    return startProcess();
}

bool BrowserHost::startProcess()
{
    const QString exe = browserExecutablePath();
    if (!QFileInfo::exists(exe)) {
        QTextStream(stderr) << "rustcam-browser not found: " << exe << "\n";
        return false;
    }

    m_stopping = false;
    m_sentRequests.clear();
    m_process = new QProcess(this);
    connect(m_process, &QProcess::errorOccurred,
            this, [this](QProcess::ProcessError) {
                if (!m_stopping) {
                    scheduleRestart();
                }
            });
            connect(m_process, QOverload<int, QProcess::ExitStatus>::of(&QProcess::finished),
            this, [this](int, QProcess::ExitStatus) {
                if (!m_stopping) {
                    scheduleRestart();
                }
            });

    m_process->start(exe, QStringList() << QStringLiteral("--pipe") << pipeName());
    if (!m_process->waitForStarted(10000)) {
        return false;
    }
    // 握手是异步的：hello.ok 到达后由 connected() 触发 sendPendingSessions()。
    m_client->connectToServer(pipeName());
    return true;
}

void BrowserHost::sendPendingSessions()
{
    for (auto it = m_pendingRequests.constBegin();
         it != m_pendingRequests.constEnd(); ++it) {
        if (m_sentRequests.contains(it.key())) {
            continue;
        }
        sendCreateSession(it.value());
        m_sentRequests.insert(it.key());
    }
}

void BrowserHost::sendCreateSession(const SessionRequest& request)
{
    QJsonObject host;
    host.insert(QStringLiteral("kind"), QString::fromLatin1(browserHostKind()));
    // WId 本身就是整数类型（quintptr），直接 static_cast，不能 reinterpret_cast。
    host.insert(QStringLiteral("handle"), encodeHandle(static_cast<quint64>(request.hostHandle)));

    QJsonObject bounds;
    bounds.insert(QStringLiteral("x"), 0);
    bounds.insert(QStringLiteral("y"), 0);
    bounds.insert(QStringLiteral("width"), 800);
    bounds.insert(QStringLiteral("height"), 600);

    QJsonObject message;
    message.insert(QStringLiteral("type"), QString::fromLatin1(msg::kSessionCreate));
    message.insert(QStringLiteral("session"), request.sessionId);
    message.insert(QStringLiteral("url"), request.url);
    message.insert(QStringLiteral("host"), host);
    message.insert(QStringLiteral("bounds"), bounds);
    message.insert(QStringLiteral("mode"), request.mode);
    m_client->send(message);
}

void BrowserHost::handleMessage(const QJsonObject& message)
{
    const QString type = message.value(QStringLiteral("type")).toString();
    const QString sessionId = message.value(QStringLiteral("session")).toString();

    if (type == QLatin1String(msg::kSessionCreated)) {
        quint64 handle = 0;
        decodeHandle(message.value(QStringLiteral("hwnd")).toString(), &handle);
        emit sessionCreated(sessionId, handle);
        return;
    }
    if (type == QLatin1String(msg::kSessionReady)) {
        emit sessionReady(sessionId);
        return;
    }
    if (type == QLatin1String(msg::kSessionEvent)) {
        emit sessionEvent(sessionId,
                          message.value(QStringLiteral("payload")).toString());
        return;
    }
    if (type == QLatin1String(msg::kSessionClosed)) {
        emit sessionClosed(sessionId);
        return;
    }
    if (type == QLatin1String(msg::kSessionFailed)) {
        emit sessionFailed(sessionId,
                           message.value(QStringLiteral("error")).toString());
        return;
    }
}

void BrowserHost::scheduleRestart()
{
    if (m_restartTimer->isActive()) {
        return;
    }
    if (m_restartAttempt >= kMaxRestartAttempts) {
        QTextStream(stderr) << "rustcam-browser restart limit reached\n";
        emit browserProcessCrashed();
        return;
    }
    const int index = qMin(m_restartAttempt, kRestartBackoffCount - 1);
    ++m_restartAttempt;
    m_restartTimer->start(kRestartBackoffMs[index]);
}

void BrowserHost::restartNow()
{
    if (m_stopping) {
        return;
    }
    if (m_process) {
        // 先断开信号，避免 kill 触发的 finished 再次排一次重启。
        m_process->disconnect(this);
        m_process->kill();
        m_process->deleteLater();
        m_process = nullptr;
    }
    m_client->disconnectFromServer();
    m_sentRequests.clear();

    if (!startProcess()) {
        scheduleRestart();
        return;
    }
    // 会话在新连接握手完成后由 sendPendingSessions() 统一重建（spec §7.1）。
}

void BrowserHost::shutdownBrowser()
{
    m_stopping = true;
    if (!m_client->isConnected()) {
        return;
    }
    QJsonObject message;
    message.insert(QStringLiteral("type"), QString::fromLatin1(msg::kShutdown));
    m_client->send(message);
    m_client->disconnectFromServer();
    if (m_process && m_process->state() != QProcess::NotRunning) {
        m_process->terminate();
        if (!m_process->waitForFinished(3000)) {
            m_process->kill();
        }
    }
}

} // namespace rustcam
