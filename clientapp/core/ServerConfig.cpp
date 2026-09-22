#include "ServerConfig.h"
#include "Logger.h"
#include <sqlite3.h>
#include <QFile>
#include <QDir>
#include <QCoreApplication>
#include <QJsonDocument>

namespace rustcam {

ServerConfig::ServerConfig(QObject* parent)
    : QObject(parent)
    , m_port(8080)
    , m_useHttps(false)
{
    m_host = "localhost";
    load();
}

ServerConfig::~ServerConfig()
{
}

ServerConfig* ServerConfig::instance()
{
    static ServerConfig inst;
    return &inst;
}

QString ServerConfig::serverHost() const
{
    return m_host;
}

int ServerConfig::serverPort() const
{
    return m_port;
}

bool ServerConfig::useHttps() const
{
    return m_useHttps;
}

QString ServerConfig::serverUrl() const
{
    QString protocol = m_useHttps ? "https" : "http";
    return QString("%1://%2:%3").arg(protocol).arg(m_host).arg(m_port);
}

void ServerConfig::setServerHost(const QString& host)
{
    if (m_host != host) {
        m_host = host;
        save();
        emit configChanged();
    }
}

void ServerConfig::setServerPort(int port)
{
    if (m_port != port) {
        m_port = port;
        save();
        emit configChanged();
    }
}

void ServerConfig::setUseHttps(bool useHttps)
{
    if (m_useHttps != useHttps) {
        m_useHttps = useHttps;
        save();
        emit configChanged();
    }
}

QString dbPath()
{
    return QCoreApplication::applicationDirPath() + "/config/rustcam.db";
}

bool ServerConfig::initDatabase()
{
    QDir dir = QFileInfo(dbPath()).absoluteDir();
    if (!dir.exists()) {
        dir.mkpath(".");
    }

    sqlite3* db = nullptr;
    if (sqlite3_open(dbPath().toUtf8().constData(), &db) != SQLITE_OK) {
        LOG_ERROR("ServerConfig", QString("Failed to open database: %1").arg(sqlite3_errmsg(db)));
        return false;
    }

    const char* sql = R"(
        CREATE TABLE IF NOT EXISTS server_config (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            host TEXT NOT NULL DEFAULT 'localhost',
            port INTEGER NOT NULL DEFAULT 8080,
            use_https INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT DEFAULT CURRENT_TIMESTAMP
        )
    )";

    char* errMsg = nullptr;
    if (sqlite3_exec(db, sql, nullptr, nullptr, &errMsg) != SQLITE_OK) {
        LOG_ERROR("ServerConfig", QString("Failed to create table: %1").arg(errMsg));
        sqlite3_free(errMsg);
        sqlite3_close(db);
        return false;
    }

    sqlite3_close(db);
    return true;
}

bool ServerConfig::load()
{
    if (!initDatabase()) {
        LOG_WARNING("ServerConfig", "Failed to init database, using defaults");
        return false;
    }

    sqlite3* db = nullptr;
    if (sqlite3_open(dbPath().toUtf8().constData(), &db) != SQLITE_OK) {
        LOG_ERROR("ServerConfig", QString("Failed to open database: %1").arg(sqlite3_errmsg(db)));
        return false;
    }

    const char* sql = "SELECT host, port, use_https FROM server_config WHERE id = 1";
    sqlite3_stmt* stmt = nullptr;

    if (sqlite3_prepare_v2(db, sql, -1, &stmt, nullptr) != SQLITE_OK) {
        LOG_ERROR("ServerConfig", QString("Failed to prepare statement: %1").arg(sqlite3_errmsg(db)));
        sqlite3_close(db);
        return false;
    }

    bool loaded = false;
    if (sqlite3_step(stmt) == SQLITE_ROW) {
        const char* hostStr = reinterpret_cast<const char*>(sqlite3_column_text(stmt, 0));
        m_host = hostStr ? QString::fromUtf8(hostStr) : "localhost";
        m_port = sqlite3_column_int(stmt, 1);
        m_useHttps = sqlite3_column_int(stmt, 2) != 0;
        loaded = true;
        LOG_INFO("ServerConfig", QString("Loaded config: %1 (HTTPS: %2)").arg(serverUrl()).arg(m_useHttps));
    } else {
        sqlite3_finalize(stmt);

        const char* insertSql = "INSERT INTO server_config (id, host, port, use_https) VALUES (1, 'localhost', 8080, 0)";
        if (sqlite3_exec(db, insertSql, nullptr, nullptr, nullptr) == SQLITE_OK) {
            m_host = "localhost";
            m_port = 8080;
            m_useHttps = false;
            loaded = true;
            LOG_INFO("ServerConfig", "Initialized default config");
        }
    }

    sqlite3_finalize(stmt);
    sqlite3_close(db);
    return loaded;
}

bool ServerConfig::save()
{
    sqlite3* db = nullptr;
    if (sqlite3_open(dbPath().toUtf8().constData(), &db) != SQLITE_OK) {
        LOG_ERROR("ServerConfig", QString("Failed to open database: %1").arg(sqlite3_errmsg(db)));
        return false;
    }

    const char* sql = "UPDATE server_config SET host = ?, port = ?, use_https = ?, updated_at = CURRENT_TIMESTAMP WHERE id = 1";
    sqlite3_stmt* stmt = nullptr;

    if (sqlite3_prepare_v2(db, sql, -1, &stmt, nullptr) != SQLITE_OK) {
        LOG_ERROR("ServerConfig", QString("Failed to prepare update: %1").arg(sqlite3_errmsg(db)));
        sqlite3_close(db);
        return false;
    }

    sqlite3_bind_text(stmt, 1, m_host.toUtf8().constData(), -1, SQLITE_TRANSIENT);
    sqlite3_bind_int(stmt, 2, m_port);
    sqlite3_bind_int(stmt, 3, m_useHttps ? 1 : 0);

    bool saved = sqlite3_step(stmt) == SQLITE_DONE;
    if (!saved) {
        LOG_ERROR("ServerConfig", QString("Failed to save config: %1").arg(sqlite3_errmsg(db)));
    } else {
        LOG_INFO("ServerConfig", QString("Saved config: %1").arg(serverUrl()));
    }

    sqlite3_finalize(stmt);
    sqlite3_close(db);
    return saved;
}

}
