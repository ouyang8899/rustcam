#include "ServerApi.h"
#include "ServerConfig.h"
#include "Logger.h"
#include <curl/curl.h>
#include <sstream>
#include <cstring>
#include <QJsonDocument>
#include <QJsonParseError>

namespace rustcam {

struct ResponseData {
    QByteArray body;
    QMap<QString, QString> headers;
};

ServerApi::ServerApi(QObject* parent)
    : QObject(parent)
{
}

ServerApi::~ServerApi()
{
}

ServerApi* ServerApi::instance()
{
    static ServerApi inst;
    return &inst;
}

void ServerApi::setAuthToken(const QString& token)
{
    {
        QMutexLocker locker(&m_tokenMutex);
        m_authToken = token;
    }
    LOG_INFO("ServerApi", QString("setAuthToken called, token length: %1").arg(token.length()));
}

void ServerApi::clearAuthToken()
{
    QMutexLocker locker(&m_tokenMutex);
    m_authToken.clear();
}

QString ServerApi::buildUrl(const QString& endpoint)
{
    return ServerConfig::instance()->serverUrl() + endpoint;
}

void ServerApi::get(const QString& endpoint, Callback callback)
{
    request(HttpMethod::Get, endpoint, QJsonObject(), callback);
}

bool ServerApi::getSync(const QString& endpoint, QJsonObject& response, QString& error)
{
    QString url = buildUrl(endpoint);
    bool success = performRequest(HttpMethod::Get, url, QJsonObject(), response, error);
    return success;
}

void ServerApi::post(const QString& endpoint, const QJsonObject& body, Callback callback)
{
    request(HttpMethod::Post, endpoint, body, callback);
}

void ServerApi::put(const QString& endpoint, const QJsonObject& body, Callback callback)
{
    request(HttpMethod::Put, endpoint, body, callback);
}

void ServerApi::deleteResource(const QString& endpoint, Callback callback)
{
    request(HttpMethod::Delete, endpoint, QJsonObject(), callback);
}

void ServerApi::request(HttpMethod method, const QString& endpoint,
                        const QJsonObject& body, Callback callback)
{
    QString url = buildUrl(endpoint);
    QJsonObject response;
    QString error;

    emit requestStarted();

    bool success = performRequest(method, url, body, response, error);

    emit requestFinished();

    if (callback) {
        callback(success, response, error);
    }
}

bool ServerApi::performRequest(HttpMethod method, const QString& url,
                               const QJsonObject& body,
                               QJsonObject& response, QString& error)
{
    CURL* curl = curl_easy_init();
    if (!curl) {
        error = "Failed to initialize curl";
        LOG_ERROR("ServerApi", error);
        return false;
    }

    ResponseData respData;
    struct curl_slist* headers = nullptr;

    headers = curl_slist_append(headers, "Content-Type: application/json");
    headers = curl_slist_append(headers, "Accept: application/json");
    headers = curl_slist_append(headers, "User-Agent: rustcam-client/1.0");

    QString authToken;
    {
        QMutexLocker locker(&m_tokenMutex);
        authToken = m_authToken;
    }
    if (!authToken.isEmpty()) {
        std::string authHeader = "Authorization: Bearer " + authToken.toStdString();
        headers = curl_slist_append(headers, authHeader.c_str());
        LOG_DEBUG("ServerApi", QString("Using auth token, length: %1").arg(authToken.length()));
    } else {
        LOG_DEBUG("ServerApi", "No auth token available");
    }


    curl_easy_setopt(curl, CURLOPT_HTTPHEADER, headers);
    curl_easy_setopt(curl, CURLOPT_URL, url.toUtf8().constData());
    curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, writeCallback);
    curl_easy_setopt(curl, CURLOPT_WRITEDATA, &respData);
    curl_easy_setopt(curl, CURLOPT_HEADERFUNCTION, headerCallback);
    curl_easy_setopt(curl, CURLOPT_HEADERDATA, &respData);

    curl_easy_setopt(curl, CURLOPT_TIMEOUT, 30L);
    curl_easy_setopt(curl, CURLOPT_CONNECTTIMEOUT, 10L);
    curl_easy_setopt(curl, CURLOPT_FOLLOWLOCATION, 1L);
    curl_easy_setopt(curl, CURLOPT_SSL_VERIFYPEER, 0L);
    curl_easy_setopt(curl, CURLOPT_SSL_VERIFYHOST, 0L);

    switch (method) {
        case HttpMethod::Get:
            break;
        case HttpMethod::Post:
            curl_easy_setopt(curl, CURLOPT_POST, 1L);
            break;
        case HttpMethod::Put:
            curl_easy_setopt(curl, CURLOPT_CUSTOMREQUEST, "PUT");
            break;
        case HttpMethod::Delete:
            curl_easy_setopt(curl, CURLOPT_CUSTOMREQUEST, "DELETE");
            break;
    }

    std::string postFields;
    if (method != HttpMethod::Get) {
        // 必须显式给出正文：libcurl 在只设置 CURLOPT_POST 时会启用默认读回调从 stdin 取正文，
        // 在 GUI 线程上会永久阻塞在 fread()（整个界面卡死）。空 body 也按 "{}" 发送。
        const QByteArray payload = body.isEmpty()
            ? QByteArrayLiteral("{}")
            : QJsonDocument(body).toJson(QJsonDocument::Compact);
        postFields = payload.toStdString();
        curl_easy_setopt(curl, CURLOPT_POSTFIELDS, postFields.c_str());
        curl_easy_setopt(curl, CURLOPT_POSTFIELDSIZE, static_cast<long>(postFields.size()));
    }

    CURLcode res = curl_easy_perform(curl);

    if (res != CURLE_OK) {
        error = QString("curl error: %1").arg(curl_easy_strerror(res));
        LOG_ERROR("ServerApi", error);
        curl_slist_free_all(headers);
        curl_easy_cleanup(curl);
        return false;
    }

    long httpCode = 0;
    curl_easy_getinfo(curl, CURLINFO_RESPONSE_CODE, &httpCode);

    curl_slist_free_all(headers);
    curl_easy_cleanup(curl);

    if (respData.body.isEmpty()) {
        error = QString("Empty response, HTTP %1").arg(httpCode);
        LOG_WARNING("ServerApi", error);
        return false;
    }

    QJsonParseError parseError;
    QJsonDocument doc = QJsonDocument::fromJson(respData.body, &parseError);

    if (parseError.error != QJsonParseError::NoError) {
        error = QString("JSON parse error: %1").arg(parseError.errorString());
        LOG_ERROR("ServerApi", error);
        return false;
    }

    response = doc.object();
    response["_httpCode"] = httpCode;

    bool success = (httpCode >= 200 && httpCode < 300);
    if (!success) {
        QString msg = response["message"].toString();
        error = QString("HTTP %1: %2").arg(httpCode).arg(msg);
        LOG_WARNING("ServerApi", error);
    }

    LOG_INFO("ServerApi", QString("Request completed: %1 %2 -> HTTP %3, body: %4")
              .arg(static_cast<int>(method))
              .arg(url)
              .arg(httpCode)
              .arg(QString::fromUtf8(respData.body).left(500)));

    return success;
}

size_t ServerApi::writeCallback(void* contents, size_t size, size_t nmemb, void* userp)
{
    size_t realsize = size * nmemb;
    ResponseData* resp = static_cast<ResponseData*>(userp);
    resp->body.append(static_cast<const char*>(contents), realsize);
    return realsize;
}

size_t ServerApi::headerCallback(void* contents, size_t size, size_t nmemb, void* userp)
{
    size_t realsize = size * nmemb;
    ResponseData* resp = static_cast<ResponseData*>(userp);
    QString header = QString::fromUtf8(static_cast<const char*>(contents), realsize);

    int colonPos = header.indexOf(':');
    if (colonPos > 0) {
        QString key = header.left(colonPos).trimmed().toLower();
        QString value = header.mid(colonPos + 1).trimmed();
        resp->headers[key] = value;
    }
    return realsize;
}

}
