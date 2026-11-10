#include "TourScheme.h"

#include <QJsonArray>
#include <QSet>
#include <algorithm>

namespace rustcam {

namespace {

QStringList toStringList(const QJsonValue& value)
{
    QStringList result;
    const QJsonArray array = value.toArray();
    result.reserve(array.size());
    for (const QJsonValue& item : array) {
        result.append(item.toString());
    }
    return result;
}

QJsonArray fromStringList(const QStringList& list)
{
    QJsonArray array;
    for (const QString& item : list) {
        array.append(item);
    }
    return array;
}

} // namespace

bool TourSchemeCodec::fromJson(const QJsonObject& obj, TourScheme* out, QString* error)
{
    if (!out) {
        if (error) {
            *error = QStringLiteral("输出参数为空");
        }
        return false;
    }

    TourScheme scheme;
    scheme.id = static_cast<qint64>(obj.value(QStringLiteral("id")).toDouble(0));
    scheme.name = obj.value(QStringLiteral("name")).toString();
    scheme.version = obj.value(QStringLiteral("version")).toInt(0);
    scheme.layoutId = obj.value(QStringLiteral("layoutId")).toInt(0);
    scheme.dwellMs = obj.value(QStringLiteral("dwellMs")).toInt(10000);
    scheme.batchTimeoutMs = obj.value(QStringLiteral("batchTimeoutMs")).toInt(5000);
    scheme.switchGapMs = obj.value(QStringLiteral("switchGapMs")).toInt(1000);
    scheme.cycleGapMs = obj.value(QStringLiteral("cycleGapMs")).toInt(0);
    scheme.maxCycles = obj.value(QStringLiteral("maxCycles")).toInt(0);
    scheme.safetyMaxDurationMs =
        static_cast<qint64>(obj.value(QStringLiteral("safetyMaxDurationMs")).toDouble(0));
    scheme.backfillLimit = obj.value(QStringLiteral("backfillLimit")).toInt(2);

    const QJsonArray channels = obj.value(QStringLiteral("deviceChannels")).toArray();
    for (const QJsonValue& value : channels) {
        const QJsonObject item = value.toObject();
        TourChannel channel;
        channel.channelTag = item.value(QStringLiteral("channelTag")).toString();
        channel.deviceTag = item.value(QStringLiteral("deviceTag")).toString();
        channel.name = item.value(QStringLiteral("name")).toString();
        channel.regionId = item.value(QStringLiteral("regionId")).toString();
        channel.groupIds = toStringList(item.value(QStringLiteral("groupIds")));
        channel.tags = toStringList(item.value(QStringLiteral("tags")));
        channel.rtspMain = item.value(QStringLiteral("rtspMain")).toString();
        channel.rtspSub = item.value(QStringLiteral("rtspSub")).toString();
        channel.sort = item.value(QStringLiteral("sort")).toInt(0);
        scheme.channels.append(channel);
    }

    const QJsonArray locked = obj.value(QStringLiteral("lockedSet")).toArray();
    for (const QJsonValue& value : locked) {
        const QJsonObject item = value.toObject();
        LockedSlot slot;
        slot.layoutItemId = item.value(QStringLiteral("layoutItemId")).toString();
        slot.deviceTag = item.value(QStringLiteral("deviceTag")).toString();
        slot.channelTag = item.value(QStringLiteral("channelTag")).toString();
        scheme.lockedSet.append(slot);
    }

    const QJsonValue windowValue = obj.value(QStringLiteral("runWindow"));
    if (windowValue.isObject()) {
        const QJsonObject window = windowValue.toObject();
        scheme.hasRunWindow = true;
        scheme.runWindow.start = window.value(QStringLiteral("start")).toString();
        scheme.runWindow.end = window.value(QStringLiteral("end")).toString();
        scheme.runWindow.crossDay = window.value(QStringLiteral("crossDay")).toBool(false);
        const QJsonArray weekdays = window.value(QStringLiteral("weekdays")).toArray();
        for (const QJsonValue& day : weekdays) {
            scheme.runWindow.weekdays.append(day.toInt(0));
        }
    }

    *out = scheme;
    return true;
}

QJsonObject TourSchemeCodec::toJson(const TourScheme& scheme)
{
    QJsonObject obj;
    obj.insert(QStringLiteral("id"), static_cast<double>(scheme.id));
    obj.insert(QStringLiteral("name"), scheme.name);
    obj.insert(QStringLiteral("version"), scheme.version);
    obj.insert(QStringLiteral("layoutId"), scheme.layoutId);

    QJsonArray channels;
    for (const TourChannel& channel : scheme.channels) {
        QJsonObject item;
        item.insert(QStringLiteral("channelTag"), channel.channelTag);
        item.insert(QStringLiteral("deviceTag"), channel.deviceTag);
        item.insert(QStringLiteral("name"), channel.name);
        item.insert(QStringLiteral("regionId"), channel.regionId);
        item.insert(QStringLiteral("groupIds"), fromStringList(channel.groupIds));
        item.insert(QStringLiteral("tags"), fromStringList(channel.tags));
        item.insert(QStringLiteral("rtspMain"), channel.rtspMain);
        item.insert(QStringLiteral("rtspSub"), channel.rtspSub);
        item.insert(QStringLiteral("sort"), channel.sort);
        channels.append(item);
    }
    obj.insert(QStringLiteral("deviceChannels"), channels);

    QJsonArray locked;
    for (const LockedSlot& slot : scheme.lockedSet) {
        QJsonObject item;
        item.insert(QStringLiteral("layoutItemId"), slot.layoutItemId);
        item.insert(QStringLiteral("deviceTag"), slot.deviceTag);
        item.insert(QStringLiteral("channelTag"), slot.channelTag);
        locked.append(item);
    }
    obj.insert(QStringLiteral("lockedSet"), locked);

    obj.insert(QStringLiteral("dwellMs"), scheme.dwellMs);
    obj.insert(QStringLiteral("batchTimeoutMs"), scheme.batchTimeoutMs);
    obj.insert(QStringLiteral("switchGapMs"), scheme.switchGapMs);
    obj.insert(QStringLiteral("cycleGapMs"), scheme.cycleGapMs);
    if (scheme.maxCycles > 0) {
        obj.insert(QStringLiteral("maxCycles"), scheme.maxCycles);
    } else {
        obj.insert(QStringLiteral("maxCycles"), QJsonValue::Null);
    }

    if (scheme.hasRunWindow) {
        QJsonObject window;
        window.insert(QStringLiteral("start"), scheme.runWindow.start);
        window.insert(QStringLiteral("end"), scheme.runWindow.end);
        window.insert(QStringLiteral("crossDay"), scheme.runWindow.crossDay);
        QJsonArray weekdays;
        for (int day : scheme.runWindow.weekdays) {
            weekdays.append(day);
        }
        window.insert(QStringLiteral("weekdays"), weekdays);
        obj.insert(QStringLiteral("runWindow"), window);
    } else {
        obj.insert(QStringLiteral("runWindow"), QJsonValue::Null);
    }

    if (scheme.safetyMaxDurationMs > 0) {
        obj.insert(QStringLiteral("safetyMaxDurationMs"),
                   static_cast<double>(scheme.safetyMaxDurationMs));
    } else {
        obj.insert(QStringLiteral("safetyMaxDurationMs"), QJsonValue::Null);
    }
    obj.insert(QStringLiteral("backfillLimit"), scheme.backfillLimit);
    return obj;
}

bool TourSchemeCodec::validate(const TourScheme& scheme, int layoutW,
                               const QStringList& layoutItemIds, QString* error)
{
    auto fail = [&](const QString& message) {
        if (error) {
            *error = message;
        }
        return false;
    };

    if (scheme.name.trimmed().isEmpty()) {
        return fail(QStringLiteral("方案名称不能为空"));
    }
    if (scheme.channels.isEmpty()) {
        return fail(QStringLiteral("方案至少需要一个通道"));
    }
    if (scheme.lockedSet.size() > layoutW) {
        return fail(QStringLiteral("锁定数量超过布局窗口数"));
    }
    if (scheme.dwellMs < 1000 || scheme.dwellMs > 86400000) {
        return fail(QStringLiteral("dwell 必须在 1000..86400000 之间"));
    }
    if (scheme.batchTimeoutMs < 500 || scheme.batchTimeoutMs > scheme.dwellMs) {
        return fail(QStringLiteral("batchTimeout 必须在 500..dwell 之间"));
    }
    if (scheme.switchGapMs < 0 || scheme.cycleGapMs < 0) {
        return fail(QStringLiteral("间隔不能为负"));
    }
    if (scheme.backfillLimit < 0 || scheme.backfillLimit > 10) {
        return fail(QStringLiteral("补位上限必须在 0..10 之间"));
    }
    if (scheme.maxCycles == 0 && !scheme.hasRunWindow && scheme.safetyMaxDurationMs <= 0) {
        return fail(QStringLiteral("无限次数必须配置运行时段或安全兜底"));
    }
    if (scheme.hasRunWindow) {
        if (scheme.runWindow.start.isEmpty() || scheme.runWindow.end.isEmpty()) {
            return fail(QStringLiteral("运行时段不完整"));
        }
    }

    QSet<QString> channels;
    for (const TourChannel& channel : scheme.channels) {
        channels.insert(channel.ref().key());
    }

    QSet<QString> lockedItems;
    for (const LockedSlot& slot : scheme.lockedSet) {
        if (!layoutItemIds.contains(slot.layoutItemId)) {
            return fail(QStringLiteral("锁定引用了不存在的布局窗口 %1").arg(slot.layoutItemId));
        }
        if (lockedItems.contains(slot.layoutItemId)) {
            return fail(QStringLiteral("同一布局窗口不能重复锁定"));
        }
        lockedItems.insert(slot.layoutItemId);
        if (!channels.contains(slot.ref().key())) {
            return fail(QStringLiteral("锁定通道 %1 不在通道队列中").arg(slot.channelTag));
        }
    }
    return true;
}

TourSnapshot TourSchemeCodec::buildSnapshot(const TourScheme& scheme, int layoutW,
                                            const QStringList& layoutItemIds)
{
    TourSnapshot snapshot;
    snapshot.layoutW = layoutW;
    snapshot.layoutItemIds = layoutItemIds;
    snapshot.lockedSet = scheme.lockedSet;

    QSet<QString> locked;
    for (const LockedSlot& slot : scheme.lockedSet) {
        locked.insert(slot.ref().key());
    }

    QVector<TourChannel> sorted = scheme.channels;
    std::stable_sort(sorted.begin(), sorted.end(),
                     [](const TourChannel& a, const TourChannel& b) { return a.sort < b.sort; });
    for (const TourChannel& channel : sorted) {
        if (!locked.contains(channel.ref().key())) {
            snapshot.activeQueue.append(channel.ref());
        }
    }
    return snapshot;
}

} // namespace rustcam
