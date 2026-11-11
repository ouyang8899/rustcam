#include "TourApi.h"

#include <QJsonArray>
#include <QMetaObject>
#include <QPointer>
#include <QtConcurrent>

#include "core/ServerApi.h"

namespace rustcam {

namespace {

/// 服务端统一响应体：{"code":0,"message":"","data":...}
QJsonObject dataOf(const QJsonObject& response)
{
    return response.value(QStringLiteral("data")).toObject();
}

/// 构造 rows×cols 的兜底布局，窗口 id 为 cell-N。
/// 与 DataManager::loadLayouts 的兜底一致：服务端没有布局时给默认窗口，
/// 否则运行页网格会是空白。
PlayerLayout makeFallbackLayout(int id, int rows, int cols, const QString& name, bool isDefault)
{
    PlayerLayout layout;
    layout.id = id;
    layout.name = name;
    layout.rows = rows;
    layout.cols = cols;
    layout.isDefault = isDefault;
    for (int i = 0; i < rows * cols; ++i) {
        LayoutItem item;
        item.id = QStringLiteral("cell-%1").arg(i);
        item.row = i / cols;
        item.col = i % cols;
        item.rowSpan = 1;
        item.colSpan = 1;
        layout.layoutJson.append(item);
    }
    return layout;
}

} // namespace

TourApi::TourApi(QObject* parent)
    : QObject(parent)
{
}

TourApi::~TourApi() = default;

void TourApi::runAsync(const RequestFn& work, const DoneFn& done)
{
    QPointer<TourApi> guard(this);
    QtConcurrent::run([guard, work, done]() {
        bool success = false;
        QJsonObject response;
        QString error;
        work(success, response, error);
        if (!guard) {
            return;
        }
        QMetaObject::invokeMethod(guard, [guard, success, response, error, done]() {
            if (!guard) {
                return;
            }
            done(success, response, error);
        }, Qt::QueuedConnection);
    });
}

void TourApi::listSchemes(SchemeListCb cb)
{
    runAsync(
        [](bool& success, QJsonObject& response, QString& error) {
            success = ServerApi::instance()->getSync(QStringLiteral("/api/v1/tour-schemes"), response, error);
        },
        [cb](bool success, const QJsonObject& response, const QString& error) {
            QVector<TourScheme> schemes;
            if (success) {
                const QJsonArray array = response.value(QStringLiteral("data")).toArray();
                for (const QJsonValue& value : array) {
                    TourScheme scheme;
                    if (TourSchemeCodec::fromJson(value.toObject(), &scheme)) {
                        schemes.append(scheme);
                    }
                }
            }
            if (cb) {
                cb(success, schemes, success ? QString() : error);
            }
        });
}

void TourApi::getScheme(qint64 id, SchemeCb cb)
{
    const QString endpoint = QStringLiteral("/api/v1/tour-schemes/%1").arg(id);
    runAsync(
        [endpoint](bool& success, QJsonObject& response, QString& error) {
            success = ServerApi::instance()->getSync(endpoint, response, error);
        },
        [cb](bool success, const QJsonObject& response, const QString& error) {
            TourScheme scheme;
            if (success) {
                TourSchemeCodec::fromJson(dataOf(response), &scheme);
            }
            if (cb) {
                cb(success, scheme, success ? QString() : error);
            }
        });
}

void TourApi::createScheme(const TourScheme& scheme, SchemeCb cb)
{
    const QJsonObject body = TourSchemeCodec::toJson(scheme);
    runAsync(
        [body](bool& success, QJsonObject& response, QString& error) {
            ServerApi::instance()->post(
                QStringLiteral("/api/v1/tour-schemes"), body,
                [&success, &response, &error](bool ok, const QJsonObject& resp, const QString& err) {
                    success = ok;
                    response = resp;
                    error = err;
                });
        },
        [cb](bool success, const QJsonObject& response, const QString& error) {
            TourScheme created;
            if (success) {
                TourSchemeCodec::fromJson(dataOf(response), &created);
            }
            if (cb) {
                cb(success, created, success ? QString() : error);
            }
        });
}

void TourApi::updateScheme(const TourScheme& scheme, int expectedVersion, VoidCb cb)
{
    QJsonObject body = TourSchemeCodec::toJson(scheme);
    body.insert(QStringLiteral("expectedVersion"), expectedVersion);
    const qint64 schemeId = scheme.id;
    runAsync(
        [body, schemeId](bool& success, QJsonObject& response, QString& error) {
            ServerApi::instance()->put(
                QStringLiteral("/api/v1/tour-schemes/%1").arg(schemeId), body,
                [&success, &response, &error](bool ok, const QJsonObject& resp, const QString& err) {
                    success = ok;
                    response = resp;
                    error = err;
                });
        },
        [cb](bool success, const QJsonObject& response, const QString& error) {
            if (cb) {
                cb(success, success ? QString() : error);
            }
        });
}

void TourApi::deleteScheme(qint64 id, VoidCb cb)
{
    const QString endpoint = QStringLiteral("/api/v1/tour-schemes/%1").arg(id);
    runAsync(
        [endpoint](bool& success, QJsonObject& response, QString& error) {
            ServerApi::instance()->deleteResource(
                endpoint,
                [&success, &response, &error](bool ok, const QJsonObject& resp, const QString& err) {
                    success = ok;
                    response = resp;
                    error = err;
                });
        },
        [cb](bool success, const QJsonObject& response, const QString& error) {
            if (cb) {
                cb(success, success ? QString() : error);
            }
        });
}

void TourApi::fetchPlayUrl(const TourChannelRef& channel, UrlCb cb)
{
    const QString endpoint = QStringLiteral("/api/v1/channels/%1/%2/play-links")
                                 .arg(channel.deviceTag, channel.channelTag);
    runAsync(
        [endpoint](bool& success, QJsonObject& response, QString& error) {
            success = ServerApi::instance()->getSync(endpoint, response, error);
        },
        [cb](bool success, const QJsonObject& response, const QString& error) {
            QString url;
            if (success) {
                const QJsonObject data = dataOf(response);
                url = data.value(QStringLiteral("rtsp_media")).toString();
                if (url.isEmpty()) {
                    url = data.value(QStringLiteral("hls")).toString();
                }
                if (url.isEmpty()) {
                    url = data.value(QStringLiteral("flv")).toString();
                }
                if (url.isEmpty()) {
                    success = false;
                }
            }
            if (cb) {
                cb(success, url, success ? QString() : error);
            }
        });
}

void TourApi::loadLayouts(LayoutCb cb)
{
    runAsync(
        [](bool& success, QJsonObject& response, QString& error) {
            success = ServerApi::instance()->getSync(QStringLiteral("/api/v1/layouts"), response, error);
        },
        [cb](bool success, const QJsonObject& response, const QString& error) {
            QList<PlayerLayout> layouts;
            if (success) {
                const QJsonArray array = response.value(QStringLiteral("data")).toArray();
                for (const QJsonValue& value : array) {
                    layouts.append(PlayerLayout::fromJson(value.toObject()));
                }
            }
            if (layouts.isEmpty()) {
                // 服务端没有布局时给出兜底窗口，否则运行页网格会是空白。
                layouts.append(makeFallbackLayout(1, 1, 1, QStringLiteral("1x1"), false));
                layouts.append(makeFallbackLayout(2, 2, 2, QStringLiteral("2x2"), true));
                layouts.append(makeFallbackLayout(3, 3, 3, QStringLiteral("3x3"), false));
            }
            if (cb) {
                cb(success, layouts, success ? QString() : error);
            }
        });
}

} // namespace rustcam
