#include "InstanceProfile.h"

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>

#include <cstdio>

#ifdef Q_OS_WIN
#include <windows.h>
#else
#include <cerrno>
#include <csignal>
#endif

namespace rustcam {
namespace browser {
namespace instance_profile {

namespace {

QString g_cacheRoot;

QString baseDir()
{
    return QDir(cacheRoot()).absoluteFilePath(QStringLiteral("instances"));
}

QString instanceDir()
{
    return QDir(baseDir()).absoluteFilePath(
        QString::number(QCoreApplication::applicationPid()));
}

/// 进程是否还活着。pid 被回收后无法区分「别人的进程」和「已退出」，
/// 但被删掉的目录只会是浏览器 profile，重建代价很小。
bool isProcessAlive(int pid)
{
    if (pid <= 0) {
        return false;
    }
#ifdef Q_OS_WIN
    HANDLE handle = OpenProcess(SYNCHRONIZE, FALSE, static_cast<DWORD>(pid));
    if (handle) {
        CloseHandle(handle);
        return true;
    }
    return GetLastError() != ERROR_INVALID_PARAMETER;
#else
    return ::kill(static_cast<pid_t>(pid), 0) == 0 || errno == EPERM;
#endif
}

/// 目录建不出来时必须让 clientapp 看得见，否则地图会莫名其妙起不来。
void reportIfMissing(const QString& dir)
{
    if (QFileInfo::exists(dir)) {
        return;
    }
    std::fprintf(stderr, "[InstanceProfile] cannot create directory: %s\n",
                 dir.toLocal8Bit().constData());
    std::fflush(stderr);
}

} // namespace

QString cacheRoot()
{
    return g_cacheRoot;
}

QString profileDir()
{
    return QDir(instanceDir()).absoluteFilePath(QStringLiteral("profile"));
}

QString tempDir()
{
    return QDir(instanceDir()).absoluteFilePath(QStringLiteral("temp"));
}

void prepare(const QString& cacheRoot)
{
    g_cacheRoot = cacheRoot.isEmpty()
        ? QDir(QDir::currentPath()).absoluteFilePath(QStringLiteral("browser-cache"))
        : QDir(cacheRoot).absolutePath();
    QDir().mkpath(baseDir());
    sweepStale();
    QDir().mkpath(profileDir());
    QDir().mkpath(tempDir());
    // profile 与 temp 建不出来时 Chromium 会自己建 profile，但 temp 目录不行，
    // 必须让 clientapp 看得见，否则地图会莫名其妙起不来。
    reportIfMissing(profileDir());
    reportIfMissing(tempDir());
}

void sweepStale()
{
    QDir instances(baseDir());
    if (!instances.exists()) {
        return;
    }
    const QStringList entries =
        instances.entryList(QDir::Dirs | QDir::NoDotAndDotDot);
    for (const QString& name : entries) {
        bool ok = false;
        const int pid = name.toInt(&ok);
        if (!ok || isProcessAlive(pid)) {
            continue;
        }
        QDir(instances.absoluteFilePath(name)).removeRecursively();
    }
}

void cleanup()
{
    QDir(instanceDir()).removeRecursively();
}

} // namespace instance_profile
} // namespace browser
} // namespace rustcam
