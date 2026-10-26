#pragma once

#include <QList>
#include <QString>
#include "TimelineWidget.h"

namespace rustcam {

/// 一个录像分段（服务端 recording_segments 的一行）
struct RecordingFile {
    QString id;
    QString filename;
    QString path;
    /// 播放地址：服务端已组合好的完整 http 地址，客户端直接使用
    QString url;
    int64_t size = 0;
    int64_t durationMs = 0;
    int64_t startTimeMs = 0;
    int64_t endTimeMs = 0;
    /// 服务端返回的起止时间是否可解析（不可解析的分段不能参与回放）
    bool timeValid = false;
    QString serverTag;
    SegmentType segmentType = SegmentType::Normal;
    qint64 alarmId = 0;
    /// 录像所属的设备与通道（多画面对比时用来分组）
    QString deviceTag;
    QString channelTag;
};

}
