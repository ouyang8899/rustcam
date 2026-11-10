#include "TourAuditLog.h"

#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QMutex>
#include <QStandardPaths>

namespace rustcam {

namespace {

constexpr qint64 kMaxLogBytes = 2 * 1024 * 1024;
QMutex g_mutex;

QString logDirPath()
{
    const QString base = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
    if (base.isEmpty()) {
        return QDir::currentPath() + QStringLiteral("/logs/video_tour");
    }
    return base + QStringLiteral("/logs/video_tour");
}

} // namespace

QString TourAuditLog::logFilePath()
{
    return logDirPath() + QStringLiteral("/video_tour.log");
}

void TourAuditLog::log(const QString& event, const QJsonObject& detail)
{
    const QString path = logFilePath();
    const QFileInfo info(path);
    if (info.exists() && info.size() > kMaxLogBytes) {
        QFile::remove(path + QStringLiteral(".1"));
        QFile::rename(path, path + QStringLiteral(".1"));
    }

    QDir dir(logDirPath());
    if (!dir.exists() && !dir.mkpath(QStringLiteral("."))) {
        return;
    }

    QFile file(path);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Append | QIODevice::Text)) {
        return;
    }

    QMutexLocker locker(&g_mutex);
    const QString line = QStringLiteral("%1 %2 %3\n")
                             .arg(QDateTime::currentDateTime().toString(Qt::ISODate),
                                  event,
                                  QString::fromUtf8(QJsonDocument(detail).toJson(QJsonDocument::Compact)));
    file.write(line.toUtf8());
    file.close();
}

} // namespace rustcam
