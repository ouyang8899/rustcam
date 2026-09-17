#pragma once

#include <QString>
#include <QVariant>
#include <QRect>
#include <QSize>

namespace rustcam {

class Utils {
public:
    static QString formatBytes(qint64 bytes);
    static QString formatDuration(int seconds);
    static QString formatTimestamp(qint64 timestamp);
    static QString generateUuid();

    static bool isValidUrl(const QString& url);
    static bool isValidRtspUrl(const QString& url);

    static QRect parseRect(const QString& str);
    static QSize parseSize(const QString& str);

    static QString getConfigDir();
    static QString getLogDir();
    static QString getCacheDir();
    static QString getPluginDir();

    static void ensureDirExists(const QString& path);

    static int clamp(int value, int min, int max);
    static double clamp(double value, double min, double max);

    static QString elideText(const QString& text, int maxWidth, const QFont& font);
};

}
