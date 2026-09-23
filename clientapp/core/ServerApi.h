#pragma once

#include <QObject>
#include <QString>
#include <QJsonObject>
#include <QMap>
#include <QVariantMap>
#include <functional>
#include "CoreExport.h"

namespace rustcam {

enum class HttpMethod {
    Get,
    Post,
    Put,
    Delete
};

class RUSTCAM_CORE_API ServerApi : public QObject {
    Q_OBJECT
public:
    using Callback = std::function<void(bool success, const QJsonObject& response, const QString& error)>;

    static ServerApi* instance();

    void setAuthToken(const QString& token);
    void clearAuthToken();

    void get(const QString& endpoint, Callback callback);
    bool getSync(const QString& endpoint, QJsonObject& response, QString& error);
    void post(const QString& endpoint, const QJsonObject& body, Callback callback);
    void put(const QString& endpoint, const QJsonObject& body, Callback callback);
    void deleteResource(const QString& endpoint, Callback callback);

    void request(HttpMethod method, const QString& endpoint,
                 const QJsonObject& body, Callback callback);

signals:
    void requestStarted();
    void requestFinished();

private:
    explicit ServerApi(QObject* parent = nullptr);
    ~ServerApi();
    Q_DISABLE_COPY(ServerApi)

    QString buildUrl(const QString& endpoint);
    bool performRequest(HttpMethod method, const QString& url,
                        const QJsonObject& body,
                        QJsonObject& response, QString& error);

    static size_t writeCallback(void* contents, size_t size, size_t nmemb, void* userp);
    static size_t headerCallback(void* contents, size_t size, size_t nmemb, void* userp);

    QString m_authToken;
};

}
