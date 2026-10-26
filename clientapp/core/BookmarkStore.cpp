#include "BookmarkStore.h"
#include "ConfigManager.h"
#include "common/Logger.h"
#include <QDateTime>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>

namespace rustcam {

namespace {
const char* kGroup = "playback";
const char* kKey = "bookmarks";
} // namespace

BookmarkStore* BookmarkStore::instance()
{
    static BookmarkStore store;
    return &store;
}

BookmarkStore::BookmarkStore(QObject* parent)
    : QObject(parent)
{
}

BookmarkStore::~BookmarkStore()
{
}

QList<Bookmark> BookmarkStore::load() const
{
    QList<Bookmark> result;
    const QString raw = ConfigManager::instance()->getValue(kGroup, kKey, QString()).toString();
    if (raw.isEmpty()) {
        return result;
    }
    QJsonParseError err;
    const QJsonDocument doc = QJsonDocument::fromJson(raw.toUtf8(), &err);
    if (err.error != QJsonParseError::NoError || !doc.isArray()) {
        LOG_WARNING("BookmarkStore", QString("bookmarks are corrupted, ignoring: %1").arg(err.errorString()));
        return result;
    }
    const QJsonArray array = doc.array();
    for (const QJsonValue& v : array) {
        const QJsonObject obj = v.toObject();
        Bookmark bookmark;
        bookmark.id = obj["id"].toVariant().toLongLong();
        bookmark.deviceTag = obj["device_tag"].toString();
        bookmark.channelTag = obj["channel_tag"].toString();
        bookmark.timeMs = obj["time_ms"].toVariant().toLongLong();
        bookmark.note = obj["note"].toString();
        bookmark.createdAt = obj["created_at"].toString();
        result.append(bookmark);
    }
    return result;
}

void BookmarkStore::save() const
{
    const QList<Bookmark> all = load();
    QJsonArray array;
    for (const Bookmark& b : all) {
        QJsonObject obj;
        obj["id"] = b.id;
        obj["device_tag"] = b.deviceTag;
        obj["channel_tag"] = b.channelTag;
        obj["time_ms"] = b.timeMs;
        obj["note"] = b.note;
        obj["created_at"] = b.createdAt;
        array.append(obj);
    }
    const QString raw = QString::fromUtf8(QJsonDocument(array).toJson(QJsonDocument::Compact));
    ConfigManager::instance()->setValue(kGroup, kKey, raw);
}

qint64 BookmarkStore::addBookmark(const QString& deviceTag, const QString& channelTag, int64_t timeMs, const QString& note)
{
    QList<Bookmark> all = load();

    qint64 nextId = 1;
    for (const Bookmark& b : all) {
        nextId = qMax(nextId, b.id + 1);
    }

    Bookmark bookmark;
    bookmark.id = nextId;
    bookmark.deviceTag = deviceTag;
    bookmark.channelTag = channelTag;
    bookmark.timeMs = timeMs;
    bookmark.note = note;
    bookmark.createdAt = QDateTime::currentDateTime().toString("yyyy-MM-dd hh:mm:ss");
    all.append(bookmark);

    QJsonArray array;
    for (const Bookmark& b : all) {
        QJsonObject obj;
        obj["id"] = b.id;
        obj["device_tag"] = b.deviceTag;
        obj["channel_tag"] = b.channelTag;
        obj["time_ms"] = b.timeMs;
        obj["note"] = b.note;
        obj["created_at"] = b.createdAt;
        array.append(obj);
    }
    ConfigManager::instance()->setValue(kGroup, kKey,
        QString::fromUtf8(QJsonDocument(array).toJson(QJsonDocument::Compact)));

    LOG_INFO("BookmarkStore", QString("bookmark added: device=%1 time=%2").arg(deviceTag).arg(timeMs));
    emit bookmarksChanged();
    return nextId;
}

bool BookmarkStore::removeBookmark(qint64 id)
{
    QList<Bookmark> all = load();
    QJsonArray array;
    bool removed = false;
    for (const Bookmark& b : all) {
        if (b.id == id) {
            removed = true;
            continue;
        }
        QJsonObject obj;
        obj["id"] = b.id;
        obj["device_tag"] = b.deviceTag;
        obj["channel_tag"] = b.channelTag;
        obj["time_ms"] = b.timeMs;
        obj["note"] = b.note;
        obj["created_at"] = b.createdAt;
        array.append(obj);
    }
    if (removed) {
        ConfigManager::instance()->setValue(kGroup, kKey,
            QString::fromUtf8(QJsonDocument(array).toJson(QJsonDocument::Compact)));
        emit bookmarksChanged();
    }
    return removed;
}

QList<Bookmark> BookmarkStore::bookmarks() const
{
    QList<Bookmark> all = load();
    std::sort(all.begin(), all.end(), [](const Bookmark& a, const Bookmark& b) {
        return a.timeMs < b.timeMs;
    });
    return all;
}

QList<Bookmark> BookmarkStore::bookmarksFor(const QString& deviceTag, const QString& channelTag) const
{
    QList<Bookmark> result;
    for (const Bookmark& b : bookmarks()) {
        if (b.deviceTag == deviceTag && (channelTag.isEmpty() || b.channelTag == channelTag)) {
            result.append(b);
        }
    }
    return result;
}

void BookmarkStore::clear()
{
    ConfigManager::instance()->setValue(kGroup, kKey, QString());
    emit bookmarksChanged();
}

}
