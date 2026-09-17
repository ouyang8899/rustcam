#pragma once

#include <QObject>
#include <QDebug>
#include <QFile>
#include <QDateTime>
#include <QMutex>
#include <QTextStream>
#include <QString>

namespace rustcam {

class Logger : public QObject {
    Q_OBJECT
public:
    enum Level {
        Debug = 0,
        Info = 1,
        Warning = 2,
        Error = 3,
        Critical = 4
    };

    static Logger* instance();
    void log(Level level, const QString& category, const QString& message);

    void debug(const QString& category, const QString& message);
    void info(const QString& category, const QString& message);
    void warning(const QString& category, const QString& message);
    void error(const QString& category, const QString& message);
    void critical(const QString& category, const QString& message);

    void setLogFile(const QString& filePath);
    void setMinLevel(Level level);
    void flush();

signals:
    void logMessage(int level, const QString& time, const QString& category, const QString& message);

private:
    explicit Logger(QObject* parent = nullptr);
    ~Logger();
    Q_DISABLE_COPY(Logger)

    QString levelString(Level level);
    void writeToFile(const QString& line);

    Level m_minLevel = Info;
    QFile* m_logFile = nullptr;
    QTextStream* m_stream = nullptr;
    QMutex m_mutex;
};

}

#define LOG_DEBUG(category, msg) rustcam::Logger::instance()->debug(category, msg)
#define LOG_INFO(category, msg) rustcam::Logger::instance()->info(category, msg)
#define LOG_WARNING(category, msg) rustcam::Logger::instance()->warning(category, msg)
#define LOG_ERROR(category, msg) rustcam::Logger::instance()->error(category, msg)
#define LOG_CRITICAL(category, msg) rustcam::Logger::instance()->critical(category, msg)
