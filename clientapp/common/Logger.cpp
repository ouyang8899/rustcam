#include "Logger.h"
#include <QDir>
#include <QFile>
#include <QTextStream>
#include <QDebug>
#include <QDateTime>

namespace rustcam {

Logger::Logger(QObject* parent)
    : QObject(parent)
{
}

Logger::~Logger()
{
    flush();
    if (m_stream) {
        m_stream->flush();
        delete m_stream;
        m_stream = nullptr;
    }
    if (m_logFile) {
        m_logFile->flush();
        m_logFile->close();
        delete m_logFile;
        m_logFile = nullptr;
    }
}

Logger* Logger::instance()
{
    static Logger inst;
    return &inst;
}

QString Logger::levelString(Level level)
{
    switch (level) {
        case Debug:    return "DEBUG";
        case Info:     return "INFO ";
        case Warning:  return "WARN ";
        case Error:    return "ERROR";
        case Critical: return "FATAL";
    }
    return "UNKNOWN";
}

void Logger::log(Level level, const QString& category, const QString& message)
{
    if (level < m_minLevel) {
        return;
    }

    QString timestamp = QDateTime::currentDateTime().toString("yyyy-MM-dd HH:mm:ss.zzz");
    QString line = QString("[%1] [%2] [%3] %4")
                       .arg(timestamp)
                       .arg(levelString(level))
                       .arg(category)
                       .arg(message);

    {
        QMutexLocker locker(&m_mutex);
        writeToFile(line);
    }

    if (level >= Error) {
        qCritical() << qPrintable(line);
    } else if (level == Warning) {
        qWarning() << qPrintable(line);
    } else {
        qDebug() << qPrintable(line);
    }

    emit logMessage(level, timestamp, category, message);
}

void Logger::writeToFile(const QString& line)
{
    if (m_stream) {
        *m_stream << line << "\n";
        m_stream->flush();
    }
}

void Logger::debug(const QString& category, const QString& message)
{
    log(Debug, category, message);
}

void Logger::info(const QString& category, const QString& message)
{
    log(Info, category, message);
}

void Logger::warning(const QString& category, const QString& message)
{
    log(Warning, category, message);
}

void Logger::error(const QString& category, const QString& message)
{
    log(Error, category, message);
}

void Logger::critical(const QString& category, const QString& message)
{
    log(Critical, category, message);
}

void Logger::setLogFile(const QString& filePath)
{
    QMutexLocker locker(&m_mutex);

    if (m_stream) {
        m_stream->flush();
        delete m_stream;
        m_stream = nullptr;
    }
    if (m_logFile) {
        m_logFile->flush();
        m_logFile->close();
        delete m_logFile;
        m_logFile = nullptr;
    }

    QFileInfo fileInfo(filePath);
    QDir dir = fileInfo.absoluteDir();
    if (!dir.exists()) {
        dir.mkpath(".");
    }

    m_logFile = new QFile(filePath);
    if (m_logFile->open(QIODevice::WriteOnly | QIODevice::Append | QIODevice::Text)) {
        m_stream = new QTextStream(m_logFile);
    }
}

void Logger::setMinLevel(Level level)
{
    m_minLevel = level;
}

void Logger::flush()
{
    QMutexLocker locker(&m_mutex);
    if (m_stream) {
        m_stream->flush();
    }
}

}
