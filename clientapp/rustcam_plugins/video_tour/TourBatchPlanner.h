#pragma once

#include "TourTypes.h"

#include <QSet>
#include <QVector>

namespace rustcam {

/// 批次填充结果。
struct FillResult {
    QVector<TourChannelRef> items;
    int cursor = 0;
    QSet<QString> keys;
    int scanned = 0;
};

/// 纯逻辑的批次/补位计算，便于单测覆盖。
class TourBatchPlanner {
public:
    /// 从 cursor 开始环形扫描，最多扫描队列长度 N 次，批内按通道键去重。
    static FillResult fill(const QVector<TourChannelRef>& queue, int cursor, int size);

    /// 补位：跳过本批已用通道与本轮已失败通道；取不到时返回空引用。
    /// @param newCursor 出参：取到通道后新的游标位置。
    static TourChannelRef backfill(const QVector<TourChannelRef>& queue, int cursor,
                                   const QSet<QString>& batchKeys, const QSet<QString>& failedKeys,
                                   int limit, int used, int* newCursor);
};

} // namespace rustcam
