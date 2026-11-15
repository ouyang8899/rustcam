#pragma once

#include <QJsonObject>
#include <QString>
#include "CoreExport.h"

namespace rustcam {

/// 播放地址接口：/api/v1/channels/{device_tag}/{channel_tag}/play-links
///
/// device_tag / channel_tag 里可能带分隔符，这里统一做百分号编码，
/// 免得各调用方自己拼路径拼出不同的结果。
RUSTCAM_CORE_API QString playLinksEndpoint(const QString& deviceTag, const QString& channelTag);

/// 从 play-links 响应里挑一个可播放地址。
///
/// 优先级：rtsp_media → hls → flv；一个都没有时返回空字符串。
RUSTCAM_CORE_API QString selectPlayableUrl(const QJsonObject& response);

}
