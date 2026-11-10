#pragma once

#include "TourTypes.h"

#include <QJsonObject>
#include <QList>
#include <QString>
#include <QStringList>
#include <QVector>

namespace rustcam {

/// 方案里的通道条目（服务端 device_channels 元素）。
struct TourChannel {
    QString channelTag;
    QString deviceTag;
    QString name;
    QString regionId;
    QStringList groupIds;
    QStringList tags;
    QString rtspMain;
    QString rtspSub;
    int sort = 0;

    TourChannelRef ref() const { return TourChannelRef{deviceTag, channelTag}; }
};

/// 锁定窗口：把某个布局窗口固定到某路通道。
struct LockedSlot {
    QString layoutItemId;
    QString deviceTag;
    QString channelTag;

    TourChannelRef ref() const { return TourChannelRef{deviceTag, channelTag}; }
};

/// 运行时段（本地时间）。
struct RunWindow {
    QString start;   ///< "HH:mm"
    QString end;     ///< "HH:mm"
    bool crossDay = false;
    QList<int> weekdays;  ///< 1=周一 .. 7=周日，空表示每天
};

/// 轮询方案。
struct TourScheme {
    qint64 id = 0;
    QString name;
    int version = 0;
    int layoutId = 0;
    QVector<TourChannel> channels;
    QVector<LockedSlot> lockedSet;
    int dwellMs = 10000;
    int batchTimeoutMs = 5000;
    int switchGapMs = 1000;
    int cycleGapMs = 0;
    int maxCycles = 0;              ///< 0 表示无限次
    bool hasRunWindow = false;
    RunWindow runWindow;
    qint64 safetyMaxDurationMs = 0; ///< 0 表示未设置
    int backfillLimit = 2;
};

/// 运行时快照：固化的队列与窗口信息。
struct TourSnapshot {
    int layoutW = 0;
    QStringList layoutItemIds;
    QVector<TourChannelRef> activeQueue;
    QVector<LockedSlot> lockedSet;
};

class TourSchemeCodec {
public:
    /// 解析服务端 JSON；缺失字段使用默认值。
    static bool fromJson(const QJsonObject& obj, TourScheme* out, QString* error = nullptr);
    /// 输出与服务端一致的 camelCase JSON。
    static QJsonObject toJson(const TourScheme& scheme);
    /// 与服务端一致的本地校验。
    static bool validate(const TourScheme& scheme, int layoutW,
                         const QStringList& layoutItemIds, QString* error = nullptr);
    /// 固化队列：剔除锁定通道并按 sort 排序。
    static TourSnapshot buildSnapshot(const TourScheme& scheme, int layoutW,
                                      const QStringList& layoutItemIds);
};

} // namespace rustcam
