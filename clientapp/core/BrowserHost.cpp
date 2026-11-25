#include "BrowserHost.h"

#include "BrowserIpcClient.h"

#include "common/BrowserIpcProtocol.h"
#include "common/Logger.h"
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

/// 浏览器进程启动后多久还没握手就算超时（毫秒）。
///
/// 超时只用于给界面一个交代，进程不会被杀：浏览器可能只是这台机器上冷启动
/// 慢，等它自己起来比杀掉重来快得多。
const int kStartupTimeoutMs = 180000;

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
    , m_startupTimer(new QTimer(this))
{
    m_restartTimer->setSingleShot(true);
    m_startupTimer->setSingleShot(true);
    m_startupTimer->setInterval(kStartupTimeoutMs);

    connect(m_client, &BrowserIpcClient::messageReceived,
            this, &BrowserHost::handleMessage);
    connect(m_client, &BrowserIpcClient::connected, this, [this]() {
        m_handshakeOk = true;
        m_restartAttempt = 0;
        m_intentionalDisconnect = false;
        m_startupTimer->stop();
        LOG_INFO("BrowserHost", "browser process handshake ok");
        emit browserProcessStarted();
        // 握手完成后才能下发 session.create。
        sendPendingSessions();
    });
    connect(m_client, &BrowserIpcClient::disconnected, this, [this]() {
        m_handshakeOk = false;
        m_sentRequests.clear();
        if (m_stopping || m_intentionalDisconnect) {
            // 主动断开（重启 / 退出）不算故障。
            return;
        }
        LOG_WARNING("BrowserHost", "browser process disconnected");
        scheduleRestart("ipc-disconnected");
    });
    connect(m_client, &BrowserIpcClient::waitProgress, this, [this](int elapsedMs) {
        emit browserProcessWaiting(elapsedMs);
    });
    connect(m_restartTimer, &QTimer::timeout, this, &BrowserHost::restartNow);
    connect(m_startupTimer, &QTimer::timeout, this, [this]() {
        LOG_ERROR("BrowserHost", "browser process startup timed out");
        emit browserProcessTimeout();
    });
    connect(qApp, &QCoreApplication::aboutToQuit,
            this, &BrowserHost::shutdownBrowser);
}

QString BrowserHost::pipeName() const
{
    return QStringLiteral("rustcam-browser-%1").arg(QCoreApplication::applicationPid());
}

QString BrowserHost::browserExecutablePath() const
{
    if (!m_executablePathOverride.isEmpty()) {
        return m_executablePathOverride;
    }

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

void BrowserHost::setExecutablePathOverride(const QString& path)
{
    m_executablePathOverride = path;
}

void BrowserHost::setExtraArguments(const QStringList& args)
{
    m_extraArguments = args;
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
            this, [this](QProcess::ProcessError error) {
                LOG_WARNING("BrowserHost",
                            QString("browser process error: %1").arg(static_cast<int>(error)));
                if (!m_stopping) {
                    scheduleRestart("process-error");
                }
            });
    connect(m_process, QOverload<int, QProcess::ExitStatus>::of(&QProcess::finished),
            this, [this](int code, QProcess::ExitStatus) {
                LOG_WARNING("BrowserHost",
                            QString("browser process exited, code=%1").arg(code));
                if (!m_stopping) {
                    scheduleRestart("process-exited");
                }
            });

    // 子进程的 stdout/stderr 必须有人读：QProcess 用管道接收，不读的话
    // 管道写满后子进程会阻塞在 write 上，整个浏览器就此卡死。
    connect(m_process, &QProcess::readyReadStandardOutput, this, [this]() {
        const QString text = QString::fromLocal8Bit(m_process->readAllStandardOutput());
        for (const QString& line : text.split('\n', Qt::SkipEmptyParts)) {
            LOG_INFO("BrowserHost", QString("browser stdout: %1").arg(line.trimmed()));
        }
    });
    connect(m_process, &QProcess::readyReadStandardError, this, [this]() {
        const QString text = QString::fromLocal8Bit(m_process->readAllStandardError());
        for (const QString& line : text.split('\n', Qt::SkipEmptyParts)) {
            LOG_WARNING("BrowserHost", QString("browser stderr: %1").arg(line.trimmed()));
        }
    });

    LOG_INFO("BrowserHost", QString("starting browser process: %1").arg(exe));
    m_process->start(exe, browserArguments());
    if (!m_process->waitForStarted(10000)) {
        LOG_ERROR("BrowserHost", "browser process failed to start");
        return false;
    }
    // 握手是异步的：hello.ok 到达后由 connected() 触发 sendPendingSessions()。
    // 浏览器进程初始化 CEF 可能要几十秒，这期间只等待，不重启。
    m_startupTimer->start();
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
    LOG_INFO("BrowserHost", QString("session.create sent: id=%1 mode=%2 url=%3")
                                .arg(request.sessionId, request.mode, request.url));
    m_client->send(message);
}

void BrowserHost::handleMessage(const QJsonObject& message)
{
    const QString type = message.value(QStringLiteral("type")).toString();
    const QString sessionId = message.value(QStringLiteral("session")).toString();

    if (type == QLatin1String(msg::kSessionCreated)) {
        quint64 handle = 0;
        decodeHandle(message.value(QStringLiteral("hwnd")).toString(), &handle);
        LOG_INFO("BrowserHost", QString("session.created: id=%1 hwnd=%2")
                                    .arg(sessionId).arg(handle));
        emit sessionCreated(sessionId, handle);
        return;
    }
    if (type == QLatin1String(msg::kSessionReady)) {
        LOG_INFO("BrowserHost", QString("session.ready: id=%1").arg(sessionId));
        emit sessionReady(sessionId);
        return;
    }
    if (type == QLatin1String(msg::kSessionEvent)) {
        emit sessionEvent(sessionId,
                          message.value(QStringLiteral("payload")).toString());
        return;
    }
    if (type == QLatin1String(msg::kSessionClosed)) {
        LOG_INFO("BrowserHost", QString("session.closed: id=%1").arg(sessionId));
        emit sessionClosed(sessionId);
        return;
    }
    if (type == QLatin1String(msg::kSessionFailed)) {
        LOG_ERROR("BrowserHost", QString("session.failed: id=%1 error=%2")
                                     .arg(sessionId,
                                          message.value(QStringLiteral("error")).toString()));
        emit sessionFailed(sessionId,
                           message.value(QStringLiteral("error")).toString());
        return;
    }
}

void BrowserHost::scheduleRestart(const char* reason)
{
    if (m_stopping || m_restartTimer->isActive()) {
        return;
    }
    if (m_restartAttempt >= kMaxRestartAttempts) {
        QTextStream(stderr) << "rustcam-browser restart limit reached\n";
        emit browserProcessCrashed();
        return;
    }
    const int index = qMin(m_restartAttempt, kRestartBackoffCount - 1);
    ++m_restartAttempt;
    QTextStream(stderr) << "rustcam-browser restart scheduled (" << reason
                         << ") in " << kRestartBackoffMs[index] << "ms\n";
    LOG_WARNING("BrowserHost",
                QString("restarting browser process in %1ms (attempt %2)")
                    .arg(kRestartBackoffMs[index]).arg(m_restartAttempt));
    m_restartTimer->start(kRestartBackoffMs[index]);
}

void BrowserHost::restartNow()
{
    if (m_stopping) {
        return;
    }
    killProcess();
    m_intentionalDisconnect = true;
    m_client->disconnectFromServer();
    m_sentRequests.clear();

    if (!startProcess()) {
        scheduleRestart("restart-failed");
        return;
    }
    // 会话在新连接握手完成后由 sendPendingSessions() 统一重建（spec §7.1）。
}

void BrowserHost::killProcess()
{
    if (!m_process) {
        return;
    }
    // 先断开信号，避免 kill 触发的 finished 再次排一次重启。
    m_process->disconnect(this);
    const qint64 pid = m_process->processId();
    if (m_process->state() != QProcess::NotRunning) {
        m_process->kill();
        // 必须等它真的死掉：两个浏览器进程同时持有 Chromium profile 时，
        // 后启动的那个会因为单例锁起不来。
        m_process->waitForFinished(5000);
    }
    m_process->deleteLater();
    m_process = nullptr;
    removeBrowserProfile(pid);
}

void BrowserHost::removeBrowserProfile(qint64 pid)
{
    // 浏览器进程是被 kill 掉的，它来不及删自己的 profile（几十兆）。
    // 目录名就是浏览器进程的 pid，进程死了就能删。
    if (pid <= 0) {
        return;
    }
    const QString dir = QDir(browserCacheRoot())
                            .absoluteFilePath(QStringLiteral("instances/%1").arg(pid));
    if (!QFileInfo::exists(dir)) {
        return;
    }
    if (QDir(dir).removeRecursively()) {
        LOG_INFO("BrowserHost", QString("removed browser profile: %1").arg(dir));
    }
}

QStringList BrowserHost::browserArguments() const
{
    QStringList args;
    args << QStringLiteral("--pipe") << pipeName()
         << QStringLiteral("--cache-root") << browserCacheRoot();
    args += m_extraArguments;
    return args;
}

QString BrowserHost::browserCacheRoot() const
{
    return QDir(QDir::currentPath()).absoluteFilePath(QStringLiteral("browser-cache"));
}

void BrowserHost::shutdownBrowser()
{
    m_stopping = true;
    m_startupTimer->stop();
    if (m_client->isConnected()) {
        QJsonObject message;
        message.insert(QStringLiteral("type"), QString::fromLatin1(msg::kShutdown));
        m_client->send(message);
        m_client->disconnectFromServer();
    }
    // 无论如何都要收掉浏览器进程：它持有 Chromium profile 的进程单例锁，
    // 残留的孤儿进程会让下一次启动直接失败。
    if (m_process) {
        m_process->disconnect(this);
        const qint64 pid = m_process->processId();
        if (m_process->state() != QProcess::NotRunning) {
            m_process->terminate();
            if (!m_process->waitForFinished(3000)) {
                m_process->kill();
                m_process->waitForFinished(2000);
            }
        }
        m_process->deleteLater();
        m_process = nullptr;
        removeBrowserProfile(pid);
    }
}

} // namespace rustcam
