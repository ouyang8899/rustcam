#include "PlayLinks.h"

#include <QUrl>

namespace rustcam {

QString playLinksEndpoint(const QString& deviceTag, const QString& channelTag)
{
    return QString("/api/v1/channels/%1/%2/play-links")
        .arg(QString::fromUtf8(QUrl::toPercentEncoding(deviceTag)),
             QString::fromUtf8(QUrl::toPercentEncoding(channelTag)));
}

QString selectPlayableUrl(const QJsonObject& response)
{
    const QJsonObject data = response.value("data").toObject();

    QString url = data.value("rtsp_media").toString();
    if (url.isEmpty()) {
        url = data.value("hls").toString();
    }
    if (url.isEmpty()) {
        url = data.value("flv").toString();
    }
    return url;
}

}
