#include "RecordingFileListWidget.h"
#include <QListView>
#include <QLabel>
#include <QVBoxLayout>
#include <QMouseEvent>
#include <QStyledItemDelegate>

namespace rustcam {

RecordingFileListModel::RecordingFileListModel(QObject* parent)
    : QAbstractListModel(parent)
{
}

int RecordingFileListModel::rowCount(const QModelIndex& parent) const {
    Q_UNUSED(parent)
    return m_files.size();
}

int RecordingFileListModel::columnCount(const QModelIndex& parent) const {
    Q_UNUSED(parent)
    return 1;
}

QVariant RecordingFileListModel::data(const QModelIndex& index, int role) const {
    if (!index.isValid() || index.row() >= m_files.size()) return QVariant();

    const RecordingFile& file = m_files[index.row()];
    switch (role) {
        case IdRole:
            return file.id;
        case FilenameRole:
            return file.filename;
        case PathRole:
            return file.path;
        case UrlRole:
            return file.url;
        case SizeRole:
            return file.size;
        case DurationRole:
            return file.durationMs;
        case StartTimeRole:
            return file.startTimeMs;
        case EndTimeRole:
            return file.endTimeMs;
        case MediaServerRole:
            return file.serverTag;
        case SegmentTypeRole:
            return static_cast<int>(file.segmentType);
        case AlarmIdRole:
            return file.alarmId;
        case Qt::DisplayRole:
        case Qt::UserRole + 100:
            return file.filename;
        default:
            return QVariant();
    }
}

QVariant RecordingFileListModel::headerData(int section, Qt::Orientation orientation, int role) const {
    Q_UNUSED(section)
    Q_UNUSED(orientation)
    Q_UNUSED(role)
    return QVariant();
}

void RecordingFileListModel::setFiles(const QList<RecordingFile>& files) {
    beginResetModel();
    m_files = files;
    endResetModel();
}

void RecordingFileListModel::clearFiles() {
    beginResetModel();
    m_files.clear();
    endResetModel();
}

void RecordingFileListModel::addFile(const RecordingFile& file) {
    beginInsertRows(QModelIndex(), m_files.size(), m_files.size());
    m_files.append(file);
    endInsertRows();
}

RecordingFile RecordingFileListModel::fileAt(int index) const {
    if (index >= 0 && index < m_files.size()) {
        return m_files[index];
    }
    return RecordingFile();
}

RecordingFileListWidget::RecordingFileListWidget(QWidget* parent)
    : QWidget(parent)
{
    QVBoxLayout* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(4);

    m_countLabel = new QLabel(tr("Recording Files (0)"), this);
    m_countLabel->setStyleSheet("color: #888; font-size: 12px; padding: 2px;");
    layout->addWidget(m_countLabel);

    m_model = new RecordingFileListModel(this);
    m_listView = new QListView(this);
    m_listView->setModel(m_model);
    m_listView->setUniformItemSizes(false);
    m_listView->setVerticalScrollMode(QAbstractItemView::ScrollPerPixel);
    m_listView->setItemDelegate(new QStyledItemDelegate(this));

    m_listView->setStyleSheet(R"(
        QListView {
            background-color: #1a1a1a;
            border: none;
            outline: none;
        }
        QListView::item {
            padding: 6px;
            border-bottom: 1px solid #2a2a2a;
            color: #e0e0e0;
        }
        QListView::item:selected {
            background-color: #0078D4;
        }
        QListView::item:hover {
            background-color: #2a2a2a;
        }
    )");

    connect(m_listView, &QListView::doubleClicked, this, &RecordingFileListWidget::onItemDoubleClicked);
    connect(m_listView, &QListView::clicked, this, &RecordingFileListWidget::onItemClicked);

    layout->addWidget(m_listView, 1);
}

RecordingFileListWidget::~RecordingFileListWidget() {
}

void RecordingFileListWidget::setFiles(const QList<RecordingFile>& files) {
    m_model->setFiles(files);
    m_countLabel->setText(tr("Recording Files (%1)").arg(files.size()));
}

void RecordingFileListWidget::clearFiles() {
    m_model->clearFiles();
    m_countLabel->setText(tr("Recording Files (0)"));
}

void RecordingFileListWidget::addFile(const RecordingFile& file) {
    m_model->addFile(file);
    m_countLabel->setText(tr("Recording Files (%1)").arg(m_model->rowCount()));
}

void RecordingFileListWidget::onItemDoubleClicked(const QModelIndex& index) {
    RecordingFile file = m_model->fileAt(index.row());
    emit fileDoubleClicked(file);
}

void RecordingFileListWidget::onItemClicked(const QModelIndex& index) {
    RecordingFile file = m_model->fileAt(index.row());
    emit fileClicked(file);
}

}
