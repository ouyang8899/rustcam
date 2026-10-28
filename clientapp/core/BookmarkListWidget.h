#pragma once

#include <QWidget>
#include <QList>
#include <QAbstractListModel>
#include <QListView>
#include "CoreExport.h"
#include "BookmarkStore.h"

namespace rustcam {

/// 书签列表的数据模型：只存数据，不持有任何控件指针
class RUSTCAM_CORE_API BookmarkListModel : public QAbstractListModel {
    Q_OBJECT
public:
    enum Roles {
        IdRole = Qt::UserRole + 1,
        TimeRole,
        NoteRole
    };

    explicit BookmarkListModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;

    void setBookmarks(const QList<Bookmark>& bookmarks);
    const QList<Bookmark>& bookmarks() const { return m_bookmarks; }
    /// 按行取书签（行号越界时返回 false）
    bool bookmarkAt(int row, Bookmark* out) const;

private:
    QList<Bookmark> m_bookmarks;
};

/// 书签列表：QListView + model/view 结构
/// 对外只用行号与数据标识，不传 QListWidgetItem 之类的控件指针
class RUSTCAM_CORE_API BookmarkListWidget : public QWidget {
    Q_OBJECT
public:
    explicit BookmarkListWidget(QWidget* parent = nullptr);
    ~BookmarkListWidget();

    void setBookmarks(const QList<Bookmark>& bookmarks);
    /// 当前选中行的书签（没有选中时返回 false）
    bool currentBookmark(Bookmark* out) const;
    BookmarkListModel* model() const { return m_model; }

signals:
    /// 双击了某一行（用行号标识）
    void bookmarkActivated(int row);

private slots:
    void onDoubleClicked(const QModelIndex& index);

private:
    QListView* m_listView;
    BookmarkListModel* m_model;
};

}

