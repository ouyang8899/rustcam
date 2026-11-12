#pragma once

#include <QString>
#include <QMetaType>

#include "core/PlayerLayout.h"

namespace rustcam {

/// 布局窗口的可读名称：优先用布局里配置的标签，否则用「第N行第M列」。
///
/// 布局窗口的 id 是设计器生成的 uuid，直接展示没有意义，
/// 这里统一给出人能看懂的名称。
inline QString layoutItemDisplayName(const LayoutItem& item)
{
    if (!item.label.isEmpty()) {
        return item.label;
    }
    return QStringLiteral("第%1行第%2列").arg(item.row + 1).arg(item.col + 1);
}

/// 轮询任务状态。
enum class TourTaskState {
    Idle,
    Running,
    Paused,
    Stopped,
    Completed,
    Expired,
    NoSlot,
    NoSource
};

/// 通道/窗口状态：运行页角标与通道状态表共用同一套状态。
enum class TourChannelState {
    Pending,
    Requesting,
    FirstFrame,
    Playing,
    Played,
    Failed,
    Offline,
    Skipped,
    Locked,
    Backfilling
};

/// 通道引用：(deviceTag, channelTag) 唯一确定一路设备通道。
struct TourChannelRef {
    QString deviceTag;
    QString channelTag;

    bool isEmpty() const { return deviceTag.isEmpty() || channelTag.isEmpty(); }
    QString key() const { return deviceTag + QLatin1Char('/') + channelTag; }
    bool operator==(const TourChannelRef& other) const
    {
        return deviceTag == other.deviceTag && channelTag == other.channelTag;
    }
    bool operator!=(const TourChannelRef& other) const { return !(*this == other); }
};

inline QString tourChannelStateText(TourChannelState state)
{
    switch (state) {
    case TourChannelState::Pending:    return QStringLiteral("待轮询");
    case TourChannelState::Requesting: return QStringLiteral("请求中");
    case TourChannelState::FirstFrame: return QStringLiteral("首帧加载");
    case TourChannelState::Playing:    return QStringLiteral("播放中");
    case TourChannelState::Played:     return QStringLiteral("已播");
    case TourChannelState::Failed:     return QStringLiteral("失败");
    case TourChannelState::Offline:    return QStringLiteral("离线");
    case TourChannelState::Skipped:    return QStringLiteral("已跳过");
    case TourChannelState::Locked:     return QStringLiteral("已锁定");
    case TourChannelState::Backfilling:return QStringLiteral("补位中");
    }
    return QStringLiteral("未知");
}

inline QString tourTaskStateText(TourTaskState state)
{
    switch (state) {
    case TourTaskState::Idle:      return QStringLiteral("空闲");
    case TourTaskState::Running:   return QStringLiteral("运行中");
    case TourTaskState::Paused:    return QStringLiteral("已暂停");
    case TourTaskState::Stopped:   return QStringLiteral("已停止");
    case TourTaskState::Completed: return QStringLiteral("已完成");
    case TourTaskState::Expired:   return QStringLiteral("已到期");
    case TourTaskState::NoSlot:    return QStringLiteral("无可用窗口");
    case TourTaskState::NoSource:  return QStringLiteral("无通道可用");
    }
    return QStringLiteral("未知");
}

} // namespace rustcam

Q_DECLARE_METATYPE(rustcam::TourChannelRef)
Q_DECLARE_METATYPE(rustcam::TourChannelState)
