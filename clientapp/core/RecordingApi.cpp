#include "RecordingApi.h"
#include "ServerApi.h"
#include "common/Logger.h"
#include <QDateTime>
#include <QJsonArray>
#include <QUrl>
#include <QtConcurrent>
#include <QFutureWatcher>

namespace rustcam {

namespace {

/// 一次查询里最多补多少个播放地址，避免异常数据导致请求风暴
constexpr int kMaxPlayUrlResolve = 20;

/// 请求参数里的时间格式：RFC3339（UTC）
/// 服务端用 chrono 的 DateTime<Utc> 解析查询参数，必须带时区，否则整个请求 400
const char* kApiTimeFormat = "yyyy-MM-ddTHH:mm:ss";

/// 兼容旧服务端的本地时间格式（无时区）
const char* kLegacyApiTimeFormat = "yyyy-MM-dd hh:mm:ss";

QString toApiTime(qint64 msSinceEpoch)
{
    if (msSinceEpoch <= 0) {
        return QString();
    }
    const QDateTime dt = QDateTime::fromMSecsSinceEpoch(msSinceEpoch).toUTC();
    return dt.toString(kApiTimeFormat) + "Z";
}

/// 解析接口返回的时间
QDateTime parseApiTime(const QString& value)
{
    if (value.isEmpty()) {
        return QDateTime();
    }
    // 服务端返回的是 RFC3339
    QDateTime dt = QDateTime::fromString(value, Qt::ISODate);
    if (dt.isValid()) {
        return dt;
    }
    return QDateTime::fromString(value, kLegacyApiTimeFormat);   // 兼容旧的本地时间格式
}

QString percentEncode(const QString& value)
{
    return QString::fromLatin1(QUrl::toPercentEncoding(value));
}

} // namespace

RecordingApi* RecordingApi::instance()
{
    static RecordingApi api;
    return &api;
}

RecordingApi::RecordingApi(QObject* parent)
    : QObject(parent)
{
}

RecordingApi::~RecordingApi()
{
}

RecordingFile RecordingApi::parseFile(const QJsonObject& obj)
{
    RecordingFile file;
    file.id = obj["id"].toString();
    file.path = obj["file_path"].toString();
    file.filename = QFileInfo(file.path).fileName();
    file.url = obj["play_url"].toString();
    file.size = obj["file_size"].toVariant().toLongLong();
    file.durationMs = obj["duration_secs"].toVariant().toLongLong() * 1000;
    file.serverTag = obj["server_tag"].toString();
    file.deviceTag = obj["device_tag"].toString();
    file.channelTag = obj["channel_tag"].toString();

    const QString startStr = obj["start_time"].toString();
    const QString endStr = obj["end_time"].toString();
    const QDateTime startDt = parseApiTime(startStr);
    const QDateTime endDt = parseApiTime(endStr);
    file.timeValid = startDt.isValid() && endDt.isValid() && endDt > startDt;
    file.startTimeMs = startDt.isValid() ? startDt.toMSecsSinceEpoch() : 0;
    file.endTimeMs = endDt.isValid() ? endDt.toMSecsSinceEpoch() : 0;

    const QString segType = obj["segment_type"].toString().toLower();
    if (segType == "alarm") {
        file.segmentType = SegmentType::Alarm;
    } else if (segType == "prebuffer" || segType == "pre-buffer") {
        file.segmentType = SegmentType::PreBuffer;
    } else {
        file.segmentType = SegmentType::Normal;
    }
    file.alarmId = obj["alarm_id"].toVariant().toLongLong();
    return file;
}

QList<RecordingFile> RecordingApi::parseFiles(const QJsonArray& array)
{
    QList<RecordingFile> files;
    files.reserve(array.size());
    for (const QJsonValue& v : array) {
        files.append(parseFile(v.toObject()));
    }
    return files;
}

RecordingFilePage RecordingApi::parsePage(const QJsonObject& dataObj)
{
    RecordingFilePage page;
    page.files = parseFiles(dataObj["files"].toArray());
    page.total = dataObj["total"].toInt(0);
    page.page = dataObj["page"].toInt(1);
    page.limit = dataObj["limit"].toInt(50);
    page.hasMore = dataObj["has_more"].toBool(false);
    return page;
}

void RecordingApi::queryFiles(const QString& deviceTag,
                              const QString& channelTag,
                              qint64 startMs,
                              qint64 endMs,
                              const QString& segmentType,
                              int page,
                              int limit,
                              FilesCallback callback)
{
    QStringList params;
    if (!deviceTag.isEmpty()) {
        params << QString("device_tag=%1").arg(percentEncode(deviceTag));
    }
    if (!channelTag.isEmpty()) {
        params << QString("channel_tag=%1").arg(percentEncode(channelTag));
    }
    if (startMs > 0 && endMs > startMs) {
        params << QString("start_time=%1").arg(percentEncode(toApiTime(startMs)));
        params << QString("end_time=%1").arg(percentEncode(toApiTime(endMs)));
    }
    if (!segmentType.isEmpty()) {
        params << QString("segment_type=%1").arg(percentEncode(segmentType));
    }
    params << QString("limit=%1").arg(limit);
    params << QString("page=%1").arg(page);

    const QString endpoint = QString("/api/v1/recordings/files?%1").arg(params.join("&"));

    QFuture<QPair<bool, QPair<RecordingFilePage, QString>>> future =
        QtConcurrent::run([endpoint]() {
            QJsonObject response;
            QString error;
            const bool success = ServerApi::instance()->getSync(endpoint, response, error);
            if (!success) {
                return QPair<bool, QPair<RecordingFilePage, QString>>(
                    false, QPair<RecordingFilePage, QString>(RecordingFilePage(), error));
            }
            RecordingFilePage result = RecordingApi::parsePage(response["data"].toObject());

            // 兜底：个别分段没有 play_url 时按 id 换一次地址
            int resolved = 0;
            for (RecordingFile& file : result.files) {
                if (!file.url.isEmpty() || file.id.isEmpty() || resolved >= kMaxPlayUrlResolve) {
                    continue;
                }
                const QString urlEndpoint =
                    QString("/api/v1/recordings/files/%1/play-url").arg(percentEncode(file.id));
                QJsonObject urlResponse;
                QString urlError;
                if (ServerApi::instance()->getSync(urlEndpoint, urlResponse, urlError)) {
                    const QString url = urlResponse["data"].toObject()["play_url"].toString();
                    if (!url.isEmpty()) {
                        file.url = url;
                        ++resolved;
                    }
                }
            }

            return QPair<bool, QPair<RecordingFilePage, QString>>(
                true, QPair<RecordingFilePage, QString>(result, QString()));
        });

    QFutureWatcher<QPair<bool, QPair<RecordingFilePage, QString>>>* watcher =
        new QFutureWatcher<QPair<bool, QPair<RecordingFilePage, QString>>>(this);
    connect(watcher,
            &QFutureWatcher<QPair<bool, QPair<RecordingFilePage, QString>>>::finished,
            this, [this, watcher, callback]() {
                const auto result = watcher->result();
                watcher->deleteLater();
                if (callback) {
                    callback(result.first, result.second.first, result.second.second);
                }
            });
    watcher->setFuture(future);
}

void RecordingApi::queryAlarms(const QString& deviceTag,
                              qint64 startMs,
                              qint64 endMs,
                              std::function<void(bool, const QList<AlarmEvent>&, const QString&)> callback)
{
    QStringList params;
    if (!deviceTag.isEmpty()) {
        params << QString("device_tag=%1").arg(percentEncode(deviceTag));
    }
    if (startMs > 0) {
        params << QString("start_time=%1").arg(percentEncode(toApiTime(startMs)));
    }
    if (endMs > 0) {
        params << QString("end_time=%1").arg(percentEncode(toApiTime(endMs)));
    }
    params << QString("page_size=100");

    const QString endpoint = QString("/api/v1/alarms?%1").arg(params.join("&"));

    QFuture<QPair<bool, QPair<QList<AlarmEvent>, QString>>> future =
        QtConcurrent::run([endpoint]() {
            QJsonObject response;
            QString error;
            const bool success = ServerApi::instance()->getSync(endpoint, response, error);
            if (!success) {
                return QPair<bool, QPair<QList<AlarmEvent>, QString>>(
                    false, QPair<QList<AlarmEvent>, QString>(QList<AlarmEvent>(), error));
            }
            QList<AlarmEvent> events;
            const QJsonObject dataObj = response["data"].toObject();
            const QJsonArray items = dataObj["items"].toArray();
            for (const QJsonValue& v : items) {
                const QJsonObject obj = v.toObject();
                AlarmEvent event;
                event.id = obj["id"].toVariant().toLongLong();
                event.alarmType = obj["alarm_type"].toString();
                event.description = obj["description"].toString();
                const QString timeStr = obj["alarm_time"].toString();
                event.timeMs = parseApiTime(timeStr).toMSecsSinceEpoch();
                events.append(event);
            }
            return QPair<bool, QPair<QList<AlarmEvent>, QString>>(
                true, QPair<QList<AlarmEvent>, QString>(events, QString()));
        });

    QFutureWatcher<QPair<bool, QPair<QList<AlarmEvent>, QString>>>* watcher =
        new QFutureWatcher<QPair<bool, QPair<QList<AlarmEvent>, QString>>>(this);
    connect(watcher,
            &QFutureWatcher<QPair<bool, QPair<QList<AlarmEvent>, QString>>>::finished,
            this, [this, watcher, callback]() {
                const auto result = watcher->result();
                watcher->deleteLater();
                if (callback) {
                    callback(result.first, result.second.first, result.second.second);
                }
            });
    watcher->setFuture(future);
}

void RecordingApi::resolvePlayUrl(const QString& fileId, UrlCallback callback)
{
    if (fileId.isEmpty()) {
        if (callback) {
            callback(false, QString(), tr("录像文件 ID 为空"));
        }
        return;
    }

    const QString endpoint =
        QString("/api/v1/recordings/files/%1/play-url").arg(percentEncode(fileId));

    QFuture<QPair<bool, QPair<QString, QString>>> future = QtConcurrent::run([endpoint]() {
        QJsonObject response;
        QString error;
        const bool success = ServerApi::instance()->getSync(endpoint, response, error);
        if (!success) {
            return QPair<bool, QPair<QString, QString>>(false, QPair<QString, QString>(QString(), error));
        }
        const QString url = response["data"].toObject()["play_url"].toString();
        return QPair<bool, QPair<QString, QString>>(true, QPair<QString, QString>(url, QString()));
    });

    QFutureWatcher<QPair<bool, QPair<QString, QString>>>* watcher =
        new QFutureWatcher<QPair<bool, QPair<QString, QString>>>(this);
    connect(watcher, &QFutureWatcher<QPair<bool, QPair<QString, QString>>>::finished,
            this, [this, watcher, callback]() {
                const auto result = watcher->result();
                watcher->deleteLater();
                if (callback) {
                    callback(result.first, result.second.first, result.second.second);
                }
            });
    watcher->setFuture(future);
}

}
