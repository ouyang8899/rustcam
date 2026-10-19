#include "SystemConfig.h"
#include "ConfigKeys.h"
#include "Logger.h"
#include <sqlite3.h>
#include <QFile>
#include <QDir>
#include <QCoreApplication>
#include <QJsonDocument>
#include <QJsonObject>

namespace rustcam {

namespace {
QString dbPath() {
    return QCoreApplication::applicationDirPath() + "/config/rustcam.db";
}
}

SystemConfig::SystemConfig(QObject* parent)
    : QObject(parent)
{
    load();
}

SystemConfig::~SystemConfig() {
}

SystemConfig* SystemConfig::instance() {
    static SystemConfig inst;
    return &inst;
}

QVariant SystemConfig::getValue(const QString& key, const QVariant& defaultValue) const {
    return m_config.value(key, defaultValue);
}

void SystemConfig::setValue(const QString& key, const QVariant& value) {
    if (m_config.value(key) != value) {
        m_config.insert(key, value);
        saveToDb(key, value);
        emit valueChanged(key, value);
    }
}

QStringList SystemConfig::allKeys() const {
    return m_config.keys();
}

bool SystemConfig::initDatabase() {
    QDir dir = QFileInfo(dbPath()).absoluteDir();
    if (!dir.exists()) {
        dir.mkpath(".");
    }

    sqlite3* db = nullptr;
    if (sqlite3_open(dbPath().toUtf8().constData(), &db) != SQLITE_OK) {
        LOG_ERROR("SystemConfig", QString("Failed to open database: %1").arg(sqlite3_errmsg(db)));
        return false;
    }

    const char* sql = R"(
        CREATE TABLE IF NOT EXISTS system_config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        )
    )";

    char* errMsg = nullptr;
    if (sqlite3_exec(db, sql, nullptr, nullptr, &errMsg) != SQLITE_OK) {
        LOG_ERROR("SystemConfig", QString("Failed to create table: %1").arg(errMsg));
        sqlite3_free(errMsg);
        sqlite3_close(db);
        return false;
    }

    sqlite3_close(db);
    return true;
}

void SystemConfig::loadFromDb() {
    sqlite3* db = nullptr;
    if (sqlite3_open(dbPath().toUtf8().constData(), &db) != SQLITE_OK) {
        LOG_ERROR("SystemConfig", QString("Failed to open database: %1").arg(sqlite3_errmsg(db)));
        return;
    }

    const char* sql = "SELECT key, value FROM system_config";
    sqlite3_stmt* stmt = nullptr;

    if (sqlite3_prepare_v2(db, sql, -1, &stmt, nullptr) != SQLITE_OK) {
        LOG_ERROR("SystemConfig", QString("Failed to prepare: %1").arg(sqlite3_errmsg(db)));
        sqlite3_close(db);
        return;
    }

    m_config.clear();

    while (sqlite3_step(stmt) == SQLITE_ROW) {
        const char* keyStr = reinterpret_cast<const char*>(sqlite3_column_text(stmt, 0));
        const char* valueStr = reinterpret_cast<const char*>(sqlite3_column_text(stmt, 1));

        if (keyStr && valueStr) {
            QString key = QString::fromUtf8(keyStr);
            QString value = QString::fromUtf8(valueStr);

            QJsonParseError error;
            QJsonDocument doc = QJsonDocument::fromJson(value.toUtf8(), &error);

            if (error.error == QJsonParseError::NoError && doc.isObject()) {
                m_config.insert(key, doc.object().value("v"));
            } else {
                m_config.insert(key, value);
            }
        }
    }

    sqlite3_finalize(stmt);
    sqlite3_close(db);
}

void SystemConfig::saveToDb(const QString& key, const QVariant& value) {
    sqlite3* db = nullptr;
    if (sqlite3_open(dbPath().toUtf8().constData(), &db) != SQLITE_OK) {
        return;
    }

    QString valueStr;
    if (value.type() == QVariant::Map || value.type() == QVariant::List) {
        QJsonObject obj;
        obj.insert("v", value.toJsonValue());
        valueStr = QJsonDocument(obj).toJson(QJsonDocument::Compact);
    } else {
        valueStr = value.toString();
    }

    const char* sql = "INSERT OR REPLACE INTO system_config (key, value, updated_at) VALUES (?, ?, CURRENT_TIMESTAMP)";
    sqlite3_stmt* stmt = nullptr;

    if (sqlite3_prepare_v2(db, sql, -1, &stmt, nullptr) == SQLITE_OK) {
        sqlite3_bind_text(stmt, 1, key.toUtf8().constData(), -1, SQLITE_TRANSIENT);
        sqlite3_bind_text(stmt, 2, valueStr.toUtf8().constData(), -1, SQLITE_TRANSIENT);
        sqlite3_step(stmt);
        sqlite3_finalize(stmt);
    }

    sqlite3_close(db);
}

bool SystemConfig::load() {
    if (!initDatabase()) {
        LOG_WARNING("SystemConfig", "Failed to init database, using defaults");
        return false;
    }

    loadFromDb();

    if (m_config.isEmpty()) {
        LOG_INFO("SystemConfig", "Initializing default config");

        QMap<QString, QVariant> defaults = {
            {CFG_MEDIA_RATE_VISIBLE, true},
            {CFG_MEDIA_RATE_ALIGN, 3},
            {CFG_DEVICE_NAME_VISIBLE, true},
            {CFG_DEVICE_NAME_ALIGN, 0},
            {CFG_TIME_VISIBLE, true},
            {CFG_TIME_ALIGN, 2},
            {CFG_TIME_FORMAT, "hh:mm:ss"},
            {CFG_WATERMARK_VISIBLE, false},
            {CFG_WATERMARK_TEXT, ""},
            {CFG_OSD_FONT_SIZE, 14},
            {CFG_OSD_FONT_COLOR, "#FFFFFFFF"},
            {CFG_OSD_BG_ALPHA, 100},
            {CFG_PLUGIN_ORDER, QStringList({"live_video", "alarm", "playback", "map", "settings"})},
        };

        for (auto it = defaults.constBegin(); it != defaults.constEnd(); ++it) {
            m_config.insert(it.key(), it.value());
            saveToDb(it.key(), it.value());
        }
    }

    LOG_INFO("SystemConfig", QString("Loaded %1 config items").arg(m_config.size()));
    return true;
}

bool SystemConfig::save() {
    for (auto it = m_config.constBegin(); it != m_config.constEnd(); ++it) {
        saveToDb(it.key(), it.value());
    }
    return true;
}

}
