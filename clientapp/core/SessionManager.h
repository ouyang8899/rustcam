#pragma once

#include <QObject>
#include <QString>
#include <QJsonObject>

namespace rustcam {

class SessionManager : public QObject {
    Q_OBJECT
public:
    static SessionManager* instance();

    bool isLoggedIn() const;
    QString accessToken() const;
    QString refreshToken() const;
    QJsonObject userInfo() const;

    void setTokens(const QString& accessToken, const QString& refreshToken);
    void setUserInfo(const QJsonObject& userInfo);
    void clearSession();

    void saveToDisk();
    void loadFromDisk();

signals:
    void sessionChanged(bool loggedIn);

private:
    explicit SessionManager(QObject* parent = nullptr);
    ~SessionManager();
    Q_DISABLE_COPY(SessionManager)

    QString m_accessToken;
    QString m_refreshToken;
    QJsonObject m_userInfo;
};

}
