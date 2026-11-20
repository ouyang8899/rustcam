#pragma once

#include "TourTypes.h"

#include <QStringList>
#include <QVector>

namespace rustcam {

class TourLockOps {
public:
    struct Slot {
        QString layoutItemId;
        TourChannelRef channel;
    };

    struct LockResult {
        bool valid = false;
        QVector<TourChannelRef> queue;
        QStringList unlockedItems;
        QStringList lockedItems;
        QVector<Slot> batch;
        int removedIndex = -1;
        QString backfillItemId;
        bool noSlot = false;
    };

    struct UnlockResult {
        bool valid = false;
        QVector<TourChannelRef> queue;
        QStringList unlockedItems;
        QStringList lockedItems;
        QVector<Slot> batch;
        int insertedIndex = -1;
        TourChannelRef restoredChannel;
    };

    static LockResult lock(const QString& layoutItemId, const TourChannelRef& channel,
                           const QVector<TourChannelRef>& queue,
                           const QStringList& unlockedItems,
                           const QStringList& lockedItems,
                           const QVector<Slot>& batch);

    static UnlockResult unlock(const QString& layoutItemId, const TourChannelRef& channel,
                               const QVector<TourChannelRef>& queue,
                               const QStringList& unlockedItems,
                               const QStringList& lockedItems,
                               const QVector<Slot>& batch,
                               const QVector<TourChannelRef>& orderedChannels);
};

} // namespace rustcam
