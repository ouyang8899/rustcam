#include "TourBatchPlanner.h"

namespace rustcam {

FillResult TourBatchPlanner::fill(const QVector<TourChannelRef>& queue, int cursor, int size)
{
    FillResult result;
    const int count = queue.size();
    if (count == 0 || size <= 0) {
        result.cursor = cursor;
        return result;
    }

    int scan = 0;
    while (result.items.size() < size && scan < count) {
        const TourChannelRef channel = queue.at(cursor);
        cursor = (cursor + 1) % count;
        ++scan;
        if (!result.keys.contains(channel.key())) {
            result.keys.insert(channel.key());
            result.items.append(channel);
        }
    }
    result.cursor = cursor;
    result.scanned = scan;
    return result;
}

TourChannelRef TourBatchPlanner::backfill(const QVector<TourChannelRef>& queue, int cursor,
                                           const QSet<QString>& batchKeys,
                                           const QSet<QString>& failedKeys,
                                           int limit, int used, int* newCursor,
                                           const QSet<QString>* coveredKeys)
{
    if (used >= limit) {
        if (newCursor) {
            *newCursor = cursor;
        }
        return TourChannelRef();
    }

    const int count = queue.size();
    if (count <= 0) {
        if (newCursor) {
            *newCursor = cursor;
        }
        return TourChannelRef();
    }

    auto scanFor = [&](bool skipCovered) -> TourChannelRef {
        int next = cursor;
        for (int scan = 0; scan < count; ++scan) {
            const TourChannelRef channel = queue.at(next);
            next = (next + 1) % count;
            const bool covered = skipCovered && coveredKeys && coveredKeys->contains(channel.key());
            if (!batchKeys.contains(channel.key()) && !failedKeys.contains(channel.key())
                && !covered) {
                if (newCursor) {
                    *newCursor = next;
                }
                return channel;
            }
        }
        return TourChannelRef();
    };

    // 优先避开本轮已播放过的通道；实在没有别的通道时再复用。
    if (coveredKeys && !coveredKeys->isEmpty()) {
        const TourChannelRef preferred = scanFor(true);
        if (!preferred.isEmpty()) {
            return preferred;
        }
    }
    const TourChannelRef fallback = scanFor(false);
    if (fallback.isEmpty() && newCursor) {
        *newCursor = cursor;
    }
    return fallback;
}

} // namespace rustcam
