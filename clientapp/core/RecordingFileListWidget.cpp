#include "RecordingFileListWidget.h"
#include <QAction>
#include <QApplication>
#include <QClipboard>
#include <QComboBox>
#include <QGuiApplication>
#include <QDateTime>
#include <QDir>
#include <QLabel>
#include <QListView>
#include <QMenu>
#include <QMouseEvent>
#include <QPainter>
#include <QStyledItemDelegate>
#include <QVBoxLayout>
#include <QJsonDocument>
#include <QJsonObject>

namespace rustcam {

namespace {

QString formatDuration(qint64 ms)
{
    if (ms <= 0) {
        return QStringLiteral("00:00");
    }
    const qint64 totalSecs = ms / 1000;
    const qint64 h = totalSecs / 3600;
    const qint64 m = (totalSecs % 3600) / 60;
    const qint64 s = totalSecs % 60;
    if (h > 0) {
        return QString("%1:%2:%3")
            .arg(h)
            .arg(m, 2, 10, QChar('0'))
            .arg(s, 2, 10, QChar('0'));
    }
    return QString("%1:%2").arg(m).arg(s, 2, 10, QChar('0'));
}

QString formatSize(qint64 bytes)
{
    if (bytes <= 0) {
        return QStringLiteral("-");
    }
    if (bytes >= 1024LL * 1024 * 1024) {
        return QString("%1 GB").arg(bytes / (1024.0 * 1024.0 * 1024.0), 0, 'f', 1);
    }
    if (bytes >= 1024LL * 1024) {
        return QString("%1 MB").arg(bytes / (1024.0 * 1024.0), 0, 'f', 1);
    }
    if (bytes >= 1024LL) {
        return QString("%1 KB").arg(bytes / 1024.0, 0, 'f', 0);
    }
    return QString("%1 B").arg(bytes);
}

QString segmentTypeText(SegmentType type)
{
    switch (type) {
    case SegmentType::Alarm:
        return QObject::tr("报警");
    case SegmentType::PreBuffer:
        return QObject::tr("预录");
    default:
        return QString();
    }
}

QColor segmentTypeColor(SegmentType type)
{
    switch (type) {
    case SegmentType::Alarm:
        return QColor("#ff6b6b");
    case SegmentType::PreBuffer:
        return QColor("#ffa726");
    default:
        return QColor("#888888");
    }
}

} // namespace

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
        case Qt::ToolTipRole:
            return QString("%1\n%2\n%3")
                .arg(file.path)
                .arg(formatDuration(file.durationMs))
                .arg(formatSize(file.size));
        case Qt::DisplayRole:
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

Qt::ItemFlags RecordingFileListModel::flags(const QModelIndex& index) const {
    Qt::ItemFlags result = QAbstractListModel::flags(index);
    if (index.isValid() && index.row() >= 0 && index.row() < m_files.size()) {
        // 允许把录像文件拖到播放窗口
        result |= Qt::ItemIsDragEnabled;
    }
    return result;
}

QMimeData* RecordingFileListModel::mimeData(const QModelIndexList& indexes) const {
    QMimeData* mime = new QMimeData();
    for (const QModelIndex& index : indexes) {
        if (!index.isValid() || index.row() < 0 || index.row() >= m_files.size()) {
            continue;
        }
        const RecordingFile& file = m_files[index.row()];
        QJsonObject obj;
        obj["channel_key"] = m_channelKey;
        obj["file_id"] = file.id;
        obj["start_time_ms"] = static_cast<double>(file.startTimeMs);
        obj["end_time_ms"] = static_cast<double>(file.endTimeMs);
        QJsonDocument doc(obj);
        mime->setData("application/x-rustcam-recording", doc.toJson());
        break;
    }
    return mime;
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

// ── 两行显示的条目 ──────────────────────────────────────────────

RecordingFileDelegate::RecordingFileDelegate(QObject* parent)
    : QStyledItemDelegate(parent)
{
}

void RecordingFileDelegate::paint(QPainter* painter,
                                 const QStyleOptionViewItem& option,
                                 const QModelIndex& index) const
{
    painter->save();
    painter->setRenderHint(QPainter::Antialiasing, false);

    QStyleOptionViewItem opt = option;
    initStyleOption(&opt, index);
    opt.text.clear();
    const QWidget* w = opt.widget;
    QStyle* style = w ? w->style() : QApplication::style();
    style->drawControl(QStyle::CE_ItemViewItem, &opt, painter, opt.widget);

    const qint64 startMs = index.data(RecordingFileListModel::StartTimeRole).toLongLong();
    const qint64 endMs = index.data(RecordingFileListModel::EndTimeRole).toLongLong();
    const qint64 duration = index.data(RecordingFileListModel::DurationRole).toLongLong();
    const qint64 size = index.data(RecordingFileListModel::SizeRole).toLongLong();
    const QString filename = index.data(RecordingFileListModel::FilenameRole).toString();
    const SegmentType type = static_cast<SegmentType>(
        index.data(RecordingFileListModel::SegmentTypeRole).toInt());

    QFont font = option.font;
    font.setPointSizeF(qMax(8.0, font.pointSizeF() - 0.5));
    painter->setFont(font);

    const QFontMetrics fm(font);
    const int lineHeight = fm.height();
    const int left = option.rect.left() + 6;
    const int right = option.rect.right() - 6;
    const int top = option.rect.top() + 4;

    // 第一行：时间区间 + 时长
    const QDateTime startDt = QDateTime::fromMSecsSinceEpoch(startMs);
    const QDateTime endDt = QDateTime::fromMSecsSinceEpoch(endMs);
    QString timeRange;
    if (startDt.isValid() && endDt.isValid()) {
        timeRange = QString("%1 - %2")
            .arg(startDt.toString("HH:mm:ss"))
            .arg(endDt.toString("HH:mm:ss"));
    }

    QFont boldFont = font;
    boldFont.setBold(true);
    painter->setFont(boldFont);
    painter->setPen(QColor("#e0e0e0"));
    const QFontMetrics boldFm(boldFont);
    painter->drawText(QRect(left, top, right - left, lineHeight),
                      Qt::AlignLeft | Qt::AlignVCenter, timeRange);

    const QString durationText = formatDuration(duration);
    const int durationWidth = boldFm.horizontalAdvance(durationText);
    painter->drawText(QRect(right - durationWidth, top, durationWidth, lineHeight),
                      Qt::AlignRight | Qt::AlignVCenter, durationText);

    // 第二行：文件名 + 大小 + 类型
    painter->setFont(font);
    painter->setPen(QColor("#9a9a9a"));
    const int secondLine = top + lineHeight;

    int cursor = left;
    const QString typeText = segmentTypeText(type);
    int typeWidth = 0;
    if (!typeText.isEmpty()) {
        const QFont typeFont = font;
        const QFontMetrics typeFm(typeFont);
        typeWidth = typeFm.horizontalAdvance(typeText) + 10;
        painter->setPen(segmentTypeColor(type));
        painter->drawText(QRect(cursor, secondLine, typeWidth, lineHeight),
                          Qt::AlignLeft | Qt::AlignVCenter, typeText);
        cursor += typeWidth;
    }

    painter->setPen(QColor("#c8c8c8"));
    const QString sizeText = formatSize(size);
    const int sizeWidth = fm.horizontalAdvance(sizeText);
    painter->drawText(QRect(right - sizeWidth, secondLine, sizeWidth, lineHeight),
                      Qt::AlignRight | Qt::AlignVCenter, sizeText);

    const int nameWidth = right - sizeWidth - cursor - 8;
    if (nameWidth > 20) {
        painter->drawText(QRect(cursor, secondLine, nameWidth, lineHeight),
                          Qt::AlignLeft | Qt::AlignVCenter,
                          fm.elidedText(filename, Qt::ElideMiddle, nameWidth));
    }

    painter->restore();
}

QSize RecordingFileDelegate::sizeHint(const QStyleOptionViewItem& option, const QModelIndex& index) const
{
    Q_UNUSED(index)
    QFont font = option.font;
    font.setPointSizeF(qMax(8.0, font.pointSizeF() - 0.5));
    const QFontMetrics fm(font);
    return QSize(option.rect.width(), fm.height() * 2 + 10);
}

// ── 控件 ────────────────────────────────────────────────────────

RecordingFileListWidget::RecordingFileListWidget(QWidget* parent)
    : QWidget(parent)
{
    QVBoxLayout* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(4);

    QHBoxLayout* headerLayout = new QHBoxLayout();
    headerLayout->setContentsMargins(0, 0, 0, 0);
    headerLayout->setSpacing(6);

    m_countLabel = new QLabel(tr("录像文件 (0)"), this);
    m_countLabel->setStyleSheet("color: #888; font-size: 12px; padding: 2px;");
    headerLayout->addWidget(m_countLabel);
    headerLayout->addStretch(1);

    m_pageSizeCombo = new QComboBox(this);
    m_pageSizeCombo->addItem("20", 20);
    m_pageSizeCombo->addItem("50", 50);
    m_pageSizeCombo->addItem("100", 100);
    m_pageSizeCombo->addItem("200", 200);
    m_pageSizeCombo->setCurrentText(QString::number(m_pageSize));
    m_pageSizeCombo->setFixedWidth(64);
    m_pageSizeCombo->setStyleSheet("font-size: 12px;");
    m_pageSizeCombo->setToolTip(tr("每页条数"));
    connect(m_pageSizeCombo, QOverload<int>::of(&QComboBox::currentIndexChanged), this,
            [this](int index) {
                const int size = m_pageSizeCombo->itemData(index).toInt();
                if (size > 0 && size != m_pageSize) {
                    setPageSize(size);
                    emit pageSizeChanged(size);
                }
            });
    headerLayout->addWidget(m_pageSizeCombo);

    layout->addLayout(headerLayout);

    m_model = new RecordingFileListModel(this);
    m_listView = new QListView(this);
    m_listView->setModel(m_model);
    m_listView->setUniformItemSizes(false);
    m_listView->setVerticalScrollMode(QAbstractItemView::ScrollPerPixel);
    m_listView->setItemDelegate(new RecordingFileDelegate(this));
    m_listView->setContextMenuPolicy(Qt::CustomContextMenu);
    // 录像文件可以拖到播放窗口（决定那个窗口播哪个通道）
    m_listView->setDragEnabled(true);
    m_listView->setDragDropMode(QAbstractItemView::DragOnly);

    m_listView->setStyleSheet(R"(
        QListView {
            background-color: #1a1a1a;
            border: none;
            outline: none;
        }
        QListView::item {
            padding: 4px;
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
    connect(m_listView, &QListView::customContextMenuRequested, this, &RecordingFileListWidget::onContextMenu);

    layout->addWidget(m_listView, 1);
}

RecordingFileListWidget::~RecordingFileListWidget() {
}

void RecordingFileListWidget::setFiles(const QList<RecordingFile>& files) {
    m_model->setFiles(files);
    setPagingInfo(m_total > 0 ? m_total : files.size(), files.size());
}

void RecordingFileListWidget::clearFiles() {
    m_model->clearFiles();
    setPagingInfo(0, 0);
}

void RecordingFileListWidget::addFile(const RecordingFile& file) {
    m_model->addFile(file);
    setPagingInfo(m_total, m_model->rowCount());
}

void RecordingFileListWidget::setChannelKey(const QString& key) {
    m_model->setChannelKey(key);
}

QString RecordingFileListWidget::channelKey() const {
    return m_model->channelKey();
}

RecordingFile RecordingFileListWidget::currentFile() const {
    return m_model->fileAt(m_listView->currentIndex().row());
}

void RecordingFileListWidget::setPagingInfo(int total, int loaded) {
    m_total = total;
    m_countLabel->setText(tr("录像文件：共 %1 条，已加载 %2 条").arg(total).arg(loaded));
}

void RecordingFileListWidget::setPageSize(int size) {
    if (size <= 0) {
        return;
    }
    m_pageSize = size;
    const int index = m_pageSizeCombo->findText(QString::number(size));
    if (index >= 0 && m_pageSizeCombo->currentIndex() != index) {
        m_pageSizeCombo->setCurrentIndex(index);
    }
}

void RecordingFileListWidget::onItemDoubleClicked(const QModelIndex& index) {
    RecordingFile file = m_model->fileAt(index.row());
    emit fileDoubleClicked(file);
}

void RecordingFileListWidget::onItemClicked(const QModelIndex& index) {
    RecordingFile file = m_model->fileAt(index.row());
    emit fileClicked(file);
}

void RecordingFileListWidget::onContextMenu(const QPoint& pos) {
    const QModelIndex index = m_listView->indexAt(pos);
    if (!index.isValid()) {
        return;
    }
    const RecordingFile file = m_model->fileAt(index.row());

    QMenu menu(this);
    QAction* downloadAction = menu.addAction(tr("下载录像文件"));
    QAction* playAction = menu.addAction(tr("播放"));
    QAction* copyAction = menu.addAction(tr("复制文件路径"));

    QAction* chosen = menu.exec(m_listView->viewport()->mapToGlobal(pos));
    if (chosen == downloadAction) {
        emit downloadRequested(file);
    } else if (chosen == playAction) {
        emit fileDoubleClicked(file);
    } else if (chosen == copyAction) {
        QGuiApplication::clipboard()->setText(file.path);
    }
}

}
