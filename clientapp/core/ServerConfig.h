#pragma once

#include <QObject>
#include <QString>
#include <QJsonObject>

namespace rustcam {

class ServerConfig : public QObject {
    Q_OBJECT
public:
    static ServerConfig* instance();

    QString serverHost() const;
    int serverPort() const;
    QString serverUrl() const;
    bool useHttps() const;

    void setServerHost(const QString& host);
    void setServerPort(int port);
    void setUseHttps(bool useHttps);

    bool load();
    bool save();

signals:
    void configChanged();

private:
    explicit ServerConfig(QObject* parent = nullptr);
    ~ServerConfig();
    Q_DISABLE_COPY(ServerConfig)

    bool initDatabase();

    QString m_host;
    int m_port;
    bool m_useHttps;
};

}
