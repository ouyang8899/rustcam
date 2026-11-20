#include "TourLockOps.h"

namespace rustcam {

TourLockOps::LockResult TourLockOps::lock(const QString& layoutItemId,
                                           const TourChannelRef& channel,
                                           const QVector<TourChannelRef>& queue,
                                           const QStringList& unlockedItems,
                                           const QStringList& lockedItems,
                                           const QVector<Slot>& batch)
{
    LockResult result;
    if (layoutItemId.isEmpty() || channel.isEmpty()) {
        return result;
    }
    if (lockedItems.contains(layoutItemId)) {
        return result;
    }

    result.queue = queue;
    for (int i = 0; i < result.queue.size(); ++i) {
        if (result.queue.at(i) == channel) {
            result.queue.remove(i);
            result.removedIndex = i;
            break;
        }
    }

    result.unlockedItems = unlockedItems;
    result.unlockedItems.removeAll(layoutItemId);
    result.lockedItems = lockedItems;
    result.lockedItems.append(layoutItemId);

    result.batch = batch;
    for (int i = 0; i < result.batch.size(); ++i) {
        Slot& slot = result.batch[i];
        if (slot.layoutItemId == layoutItemId) {
            slot.channel = TourChannelRef();
        } else if (slot.channel == channel && result.backfillItemId.isEmpty()) {
            result.backfillItemId = slot.layoutItemId;
        }
    }

    result.noSlot = result.unlockedItems.isEmpty();
    result.valid = true;
    return result;
}

TourLockOps::UnlockResult TourLockOps::unlock(const QString& layoutItemId,
                                               const TourChannelRef& channel,
                                               const QVector<TourChannelRef>& queue,
                                               const QStringList& unlockedItems,
                                               const QStringList& lockedItems,
                                               const QVector<Slot>& batch,
                                               const QVector<TourChannelRef>& orderedChannels)
{
    UnlockResult result;
    if (!lockedItems.contains(layoutItemId)) {
        return result;
    }

    result.queue = queue;
    result.restoredChannel = channel;
    if (!channel.isEmpty()) {
        int before = 0;
        for (const TourChannelRef& ref : orderedChannels) {
            if (ref == channel) {
                break;
            }
            if (result.queue.contains(ref)) {
                ++before;
            }
        }
        if (before > result.queue.size()) {
            before = result.queue.size();
        }
        result.queue.insert(before, channel);
        result.insertedIndex = before;
    }

    result.unlockedItems = unlockedItems;
    if (!result.unlockedItems.contains(layoutItemId)) {
        result.unlockedItems.append(layoutItemId);
    }
    result.lockedItems = lockedItems;
    result.lockedItems.removeAll(layoutItemId);

    result.batch = batch;
    for (int i = 0; i < result.batch.size(); ++i) {
        Slot& slot = result.batch[i];
        if (slot.layoutItemId == layoutItemId) {
            slot.channel = TourChannelRef();
        }
    }

    result.valid = true;
    return result;
}

} // namespace rustcam
