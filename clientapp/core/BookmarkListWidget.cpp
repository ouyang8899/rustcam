#include "BookmarkListWidget.h"
#include <QDateTime>
#include <QVBoxLayout>

namespace rustcam {

BookmarkListModel::BookmarkListModel(QObject* parent)
    : QAbstractListModel(parent)
{
}

int BookmarkListModel::rowCount(const QModelIndex& parent) const {
    if (parent.isValid()) {
        return 0;
    }
    return m_bookmarks.size();
}

QVariant BookmarkListModel::data(const QModelIndex& index, int role) const {
    if (!index.isValid() || index.row() < 0 || index.row() >= m_bookmarks.size()) {
        return QVariant();
    }
    const Bookmark& bookmark = m_bookmarks[index.row()];
    switch (role) {
    case Qt::DisplayRole: {
        const QString time = QDateTime::fromMSecsSinceEpoch(bookmark.timeMs).toString("MM-dd hh:mm:ss");
        return bookmark.note.isEmpty() ? time : QString("%1  %2").arg(time, bookmark.note);
    }
    case IdRole:
        return QVariant::fromValue(bookmark.id);
    case TimeRole:
        return QVariant::fromValue(bookmark.timeMs);
    case NoteRole:
        return bookmark.note;
    default:
        return QVariant();
    }
}

void BookmarkListModel::setBookmarks(const QList<Bookmark>& bookmarks) {
    beginResetModel();
    m_bookmarks = bookmarks;
    endResetModel();
}

bool BookmarkListModel::bookmarkAt(int row, Bookmark* out) const {
    if (!out || row < 0 || row >= m_bookmarks.size()) {
        return false;
    }
    *out = m_bookmarks[row];
    return true;
}

BookmarkListWidget::BookmarkListWidget(QWidget* parent)
    : QWidget(parent)
    , m_listView(nullptr)
    , m_model(nullptr)
{
    setObjectName("BookmarkListWidget");

    QVBoxLayout* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_model = new BookmarkListModel(this);

    m_listView = new QListView(this);
    m_listView->setObjectName("PlaybackBookmarkList");
    m_listView->setModel(m_model);
    m_listView->setSelectionMode(QAbstractItemView::SingleSelection);
    m_listView->setMaximumHeight(110);
    layout->addWidget(m_listView, 1);

    // 双击用行号标识，不用 item 指针（列表刷新后指针可能已失效）
    connect(m_listView, &QListView::doubleClicked, this, &BookmarkListWidget::onDoubleClicked);
}

BookmarkListWidget::~BookmarkListWidget() {
}

void BookmarkListWidget::setBookmarks(const QList<Bookmark>& bookmarks) {
    m_model->setBookmarks(bookmarks);
}

bool BookmarkListWidget::currentBookmark(Bookmark* out) const {
    if (!out) {
        return false;
    }
    const QModelIndex index = m_listView->currentIndex();
    if (!index.isValid()) {
        return false;
    }
    return m_model->bookmarkAt(index.row(), out);
}

void BookmarkListWidget::onDoubleClicked(const QModelIndex& index) {
    if (!index.isValid()) {
        return;
    }
    emit bookmarkActivated(index.row());
}

}

