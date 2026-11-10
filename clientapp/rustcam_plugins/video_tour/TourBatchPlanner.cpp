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
                                           int limit, int used, int* newCursor)
{
    if (used >= limit) {
        if (newCursor) {
            *newCursor = cursor;
        }
        return TourChannelRef();
    }

    const int count = queue.size();
    for (int scan = 0; scan < count; ++scan) {
        const TourChannelRef channel = queue.at(cursor);
        cursor = (cursor + 1) % count;
        if (!batchKeys.contains(channel.key()) && !failedKeys.contains(channel.key())) {
            if (newCursor) {
                *newCursor = cursor;
            }
            return channel;
        }
    }
    if (newCursor) {
        *newCursor = cursor;
    }
    return TourChannelRef();
}

} // namespace rustcam
