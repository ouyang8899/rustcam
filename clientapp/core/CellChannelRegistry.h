#pragma once

#include <QMap>
#include <QString>
#include "CoreExport.h"

namespace rustcam {

/// 记录每个画面格子当前正在播放的通道。
///
/// 格子 id（cell-0 这种）和通道身份（device_tag + channel_tag）是两回事：
/// 格子表示画面位置，通道表示正在播哪一路设备视频。这里只做登记，
/// 不涉及任何界面逻辑，方便单独测试。
class RUSTCAM_CORE_API CellChannelRegistry {
public:
    /// 登记某个格子正在播放的通道；device_tag / channel_tag 任一为空时视为没有有效通道。
    void set(const QString& cellId, const QString& deviceTag, const QString& channelTag);

    /// 清除某个格子的通道登记（画面停止时调用）。
    void remove(const QString& cellId);

    /// 两个格子互换位置时，通道也跟着换。
    void swapCells(const QString& firstCellId, const QString& secondCellId);

    /// 查询某个格子正在播放的通道；没有登记时返回 false。
    bool channelOf(const QString& cellId, QString* deviceTag, QString* channelTag) const;

    void clear();

private:
    struct Entry {
        QString deviceTag;
        QString channelTag;
    };

    QMap<QString, Entry> m_entries;
};

}
