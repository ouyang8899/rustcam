#pragma once

#include <QObject>
#include <QList>
#include <QString>
#include <functional>
#include "CoreExport.h"

namespace rustcam {

/// 一条书签（回放打点）
struct Bookmark {
    qint64 id = 0;
    /// 设备/通道标识
    QString deviceTag;
    QString channelTag;
    /// 打点时间（绝对时间，毫秒）
    int64_t timeMs = 0;
    QString note;
    QString createdAt;
};

/// 书签本地存储（配置目录下的 JSON，不依赖服务端）。
class RUSTCAM_CORE_API BookmarkStore : public QObject {
    Q_OBJECT
public:
    static BookmarkStore* instance();

    /// 新增书签，返回新书签的 id
    qint64 addBookmark(const QString& deviceTag, const QString& channelTag, int64_t timeMs, const QString& note = QString());
    /// 删除书签
    bool removeBookmark(qint64 id);
    /// 全部书签（按时间排序）
    QList<Bookmark> bookmarks() const;
    /// 某个设备/通道的书签
    QList<Bookmark> bookmarksFor(const QString& deviceTag, const QString& channelTag) const;
    /// 清空
    void clear();

signals:
    void bookmarksChanged();

private:
    explicit BookmarkStore(QObject* parent = nullptr);
    ~BookmarkStore();

    void save() const;
    QList<Bookmark> load() const;
};

}
