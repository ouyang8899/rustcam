#include "SessionManager.h"
#include "Logger.h"
#include "ServerApi.h"
#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QJsonDocument>

namespace rustcam {

SessionManager::SessionManager(QObject* parent)
    : QObject(parent)
{
    loadFromDisk();
}

SessionManager::~SessionManager()
{
    saveToDisk();
}

SessionManager* SessionManager::instance()
{
    static SessionManager inst;
    return &inst;
}

bool SessionManager::isLoggedIn() const
{
    return !m_accessToken.isEmpty();
}

QString SessionManager::accessToken() const
{
    return m_accessToken;
}

QString SessionManager::refreshToken() const
{
    return m_refreshToken;
}

QJsonObject SessionManager::userInfo() const
{
    return m_userInfo;
}

void SessionManager::setTokens(const QString& accessToken, const QString& refreshToken)
{
    m_accessToken = accessToken;
    m_refreshToken = refreshToken;
    LOG_INFO("SessionManager", QString("setTokens called, token length: %1").arg(accessToken.length()));
    ServerApi::instance()->setAuthToken(accessToken);
    saveToDisk();
    emit sessionChanged(true);
    LOG_INFO("SessionManager", "Session tokens updated");
}

void SessionManager::setUserInfo(const QJsonObject& userInfo)
{
    m_userInfo = userInfo;
    saveToDisk();
}

void SessionManager::clearSession()
{
    m_accessToken.clear();
    m_refreshToken.clear();
    m_userInfo = QJsonObject();
    ServerApi::instance()->clearAuthToken();
    saveToDisk();
    emit sessionChanged(false);
    LOG_INFO("SessionManager", "Session cleared");
}

QString sessionFilePath()
{
    return QCoreApplication::applicationDirPath() + "/config/session.json";
}

void SessionManager::saveToDisk()
{
    QJsonObject obj;
    obj["access_token"] = m_accessToken;
    obj["refresh_token"] = m_refreshToken;
    obj["user_info"] = m_userInfo;

    QDir dir = QFileInfo(sessionFilePath()).absoluteDir();
    if (!dir.exists()) {
        dir.mkpath(".");
    }

    QFile file(sessionFilePath());
    if (file.open(QIODevice::WriteOnly)) {
        file.write(QJsonDocument(obj).toJson());
        file.close();
    } else {
        LOG_ERROR("SessionManager", "Failed to save session to disk");
    }
}

void SessionManager::loadFromDisk()
{
    QFile file(sessionFilePath());
    if (!file.exists()) {
        return;
    }

    if (file.open(QIODevice::ReadOnly)) {
        QJsonDocument doc = QJsonDocument::fromJson(file.readAll());
        file.close();

        QJsonObject obj = doc.object();
        m_accessToken = obj["access_token"].toString();
        m_refreshToken = obj["refresh_token"].toString();
        m_userInfo = obj["user_info"].toObject();
    }
}

}
