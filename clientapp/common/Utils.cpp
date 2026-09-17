#include "Utils.h"
#include "Logger.h"
#include <QDir>
#include <QFile>
#include <QUuid>
#include <QRegularExpression>
#include <QFontMetrics>
#include <QCoreApplication>

namespace rustcam {

QString Utils::formatBytes(qint64 bytes)
{
    const qint64 KB = 1024;
    const qint64 MB = KB * 1024;
    const qint64 GB = MB * 1024;

    if (bytes >= GB) {
        return QString::number(bytes / (double)GB, 'f', 2) + " GB";
    } else if (bytes >= MB) {
        return QString::number(bytes / (double)MB, 'f', 2) + " MB";
    } else if (bytes >= KB) {
        return QString::number(bytes / (double)KB, 'f', 2) + " KB";
    }
    return QString::number(bytes) + " B";
}

QString Utils::formatDuration(int seconds)
{
    int hours = seconds / 3600;
    int minutes = (seconds % 3600) / 60;
    int secs = seconds % 60;

    if (hours > 0) {
        return QString("%1:%2:%3")
            .arg(hours, 2, 10, QChar('0'))
            .arg(minutes, 2, 10, QChar('0'))
            .arg(secs, 2, 10, QChar('0'));
    }
    return QString("%1:%2")
        .arg(minutes, 2, 10, QChar('0'))
        .arg(secs, 2, 10, QChar('0'));
}

QString Utils::formatTimestamp(qint64 timestamp)
{
    QDateTime dt = QDateTime::fromMSecsSinceEpoch(timestamp);
    return dt.toString("yyyy-MM-dd HH:mm:ss");
}

QString Utils::generateUuid()
{
    return QUuid::createUuid().toString();
}

bool Utils::isValidUrl(const QString& url)
{
    QRegularExpression re(R"(^(https?|wss?)://[^\s/$.?#].[^\s]*$)");
    return re.match(url).hasMatch();
}

bool Utils::isValidRtspUrl(const QString& url)
{
    return url.startsWith("rtsp://", Qt::CaseInsensitive);
}

QRect Utils::parseRect(const QString& str)
{
    QStringList parts = str.split(',');
    if (parts.size() == 4) {
        return QRect(parts[0].toInt(), parts[1].toInt(),
                     parts[2].toInt(), parts[3].toInt());
    }
    return QRect();
}

QSize Utils::parseSize(const QString& str)
{
    QStringList parts = str.split('x');
    if (parts.size() == 2) {
        return QSize(parts[0].toInt(), parts[1].toInt());
    }
    return QSize();
}

QString Utils::getConfigDir()
{
    return QCoreApplication::applicationDirPath() + "/config";
}

QString Utils::getLogDir()
{
    return QCoreApplication::applicationDirPath() + "/logs";
}

QString Utils::getCacheDir()
{
    return QCoreApplication::applicationDirPath() + "/cache";
}

QString Utils::getPluginDir()
{
    return QCoreApplication::applicationDirPath() + "/plugins";
}

void Utils::ensureDirExists(const QString& path)
{
    QDir dir(path);
    if (!dir.exists()) {
        dir.mkpath(".");
    }
}

int Utils::clamp(int value, int minVal, int maxVal)
{
    return qBound(value, minVal, maxVal);
}

double Utils::clamp(double value, double minVal, double maxVal)
{
    return qBound(value, minVal, maxVal);
}

QString Utils::elideText(const QString& text, int maxWidth, const QFont& font)
{
    QFontMetrics fm(font);
    if (fm.horizontalAdvance(text) <= maxWidth) {
        return text;
    }
    return fm.elidedText(text, Qt::ElideRight, maxWidth);
}

}
