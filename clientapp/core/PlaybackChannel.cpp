#include "PlaybackChannel.h"
#include "RecordingApi.h"
#include "FileDownloader.h"
#include "RecordingFileListWidget.h"
#include "common/Logger.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QScrollArea>
#include <QLabel>
#include <QDateEdit>
#include <QComboBox>
#include <QDateTimeEdit>
#include <QListWidget>
#include <QPushButton>
#include <QFrame>
#include <QMessageBox>
#include <QDir>
#include <QFileDialog>
#include <QStandardPaths>
#include <QUrl>

namespace rustcam {

namespace {

/// 一次查询最多拉取的页数（50 页 * 每页 50 条 = 2500 段），避免异常数据导致请求风暴
constexpr int kMaxPages = 50;

/// 检索条件区域固定高度（超出部分在滚动区里滚动）
constexpr int kFilterAreaHeight = 240;

/// 表单里的一行：标签在上、控件在下
void addFieldRow(QVBoxLayout* form, const QString& text, QWidget* field)
{
    QLabel* label = new QLabel(text, field);
    label->setStyleSheet("color: #aaa;");
    QVBoxLayout* row = new QVBoxLayout();
    row->setContentsMargins(0, 0, 0, 0);
    row->setSpacing(2);
    row->addWidget(label);
    row->addWidget(field);
    form->addLayout(row);
}

}  // namespace

PlaybackChannel::PlaybackChannel(const QString& deviceTag,
                                 const QString& channelTag,
                                 const QString& displayName,
                                 QObject* parent)
    : QObject(parent)
    , m_deviceTag(deviceTag)
    , m_channelTag(channelTag)
    , m_displayName(displayName)
    , m_selectedDate(QDate::currentDate())
{
    // 时间轴由 ReplayPlayerContainer 托管显示（视频区下方）
    m_timeline = new TimelineWidget();
    connect(m_timeline, &TimelineWidget::timeSelected, this, &PlaybackChannel::seekRequested);
    connect(m_timeline, &TimelineWidget::periodClicked, this, [this](const RecordingPeriod& period) {
        emit seekRequested(period.startTimeMs);
    });

    m_downloader = new FileDownloader(this);
    connect(m_downloader, &FileDownloader::finished, this,
            [this](bool success, const QString& path, const QString& error) {
                if (success) {
                    LOG_INFO("PlaybackChannel", QString("download finished: %1").arg(path));
                    QMessageBox::information(m_hostWidget.data(), tr("下载完成"),
                        tr("录像文件已保存到：%1").arg(path));
                } else {
                    LOG_ERROR("PlaybackChannel", QString("download failed: %1").arg(error));
                    QMessageBox::warning(m_hostWidget.data(), tr("下载失败"), error);
                }
            });
}

PlaybackChannel::~PlaybackChannel() {
}

QString PlaybackChannel::key() const {
    return m_deviceTag + "/" + m_channelTag;
}

// ── 侧栏页面 ────────────────────────────────────────────────────────

QWidget* PlaybackChannel::createSidePanelPage(QWidget* parent) {
    QWidget* page = new QWidget(parent);
    m_hostWidget = page;
    QVBoxLayout* layout = new QVBoxLayout(page);
    layout->setContentsMargins(4, 4, 4, 4);
    layout->setSpacing(4);

    QLabel* filterTitle = new QLabel(tr("检索条件"), page);
    filterTitle->setStyleSheet("color: #e0e0e0; font-weight: bold; padding: 4px;");
    layout->addWidget(filterTitle);

    QFrame* filterFrame = new QFrame(page);
    filterFrame->setStyleSheet("border: 1px solid #3e3e3e;");
    QVBoxLayout* filterFrameLayout = new QVBoxLayout(filterFrame);
    filterFrameLayout->setContentsMargins(2, 2, 2, 2);
    filterFrameLayout->setSpacing(0);
    filterFrameLayout->addWidget(buildFilterArea(page));
    layout->addWidget(filterFrame, 0);

    QLabel* fileListLabel = new QLabel(tr("Recording Files"), page);
    fileListLabel->setStyleSheet("color: #e0e0e0; font-weight: bold; padding: 4px;");
    layout->addWidget(fileListLabel);

    layout->addWidget(buildFileListArea(page), 1);
    layout->addWidget(buildBookmarkArea(page), 0);

    return page;
}

QWidget* PlaybackChannel::buildFilterArea(QWidget* parent) {
    m_filterWidget = new QWidget(parent);
    QVBoxLayout* outer = new QVBoxLayout(m_filterWidget);
    outer->setContentsMargins(0, 0, 0, 0);
    outer->setSpacing(0);

    QScrollArea* scroll = new QScrollArea(m_filterWidget);
    scroll->setWidgetResizable(true);
    scroll->setFrameShape(QFrame::NoFrame);
    scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    scroll->setFixedHeight(kFilterAreaHeight);
    scroll->setStyleSheet(R"(
        QScrollArea {
            background-color: #2a2a2a;
            border: none;
        }
        QScrollArea > QWidget > QWidget {
            background-color: #2a2a2a;
        }
    )");
    outer->addWidget(scroll);

    QWidget* formHost = new QWidget(scroll);
    QVBoxLayout* form = new QVBoxLayout(formHost);
    form->setContentsMargins(4, 4, 4, 4);
    form->setSpacing(4);

    m_dateEdit = new QDateEdit(m_selectedDate, formHost);
    m_dateEdit->setCalendarPopup(true);
    m_dateEdit->setStyleSheet(R"(
        QDateEdit {
            background-color: #2a2a2a;
            border: 1px solid #3e3e3e;
            border-radius: 4px;
            padding: 4px;
            color: #e0e0e0;
        }
    )");
    connect(m_dateEdit, &QDateEdit::dateChanged, this, &PlaybackChannel::onDateChanged);
    addFieldRow(form, tr("Date:"), m_dateEdit);

    m_typeCombo = new QComboBox(formHost);
    m_typeCombo->addItem(tr("全部类型"), QString());
    m_typeCombo->addItem(tr("普通"), QStringLiteral("normal"));
    m_typeCombo->addItem(tr("报警"), QStringLiteral("alarm"));
    m_typeCombo->addItem(tr("预录"), QStringLiteral("prebuffer"));
    m_typeCombo->setToolTip(tr("按录像分段类型过滤"));
    connect(m_typeCombo, QOverload<int>::of(&QComboBox::currentIndexChanged), this, [this](int) {
        onSegmentTypeChanged(m_typeCombo->currentData().toString());
    });
    addFieldRow(form, tr("Type:"), m_typeCombo);

    m_startTimeEdit = new QDateTimeEdit(QDateTime(m_selectedDate, QTime(0, 0, 0)), formHost);
    m_endTimeEdit = new QDateTimeEdit(QDateTime(m_selectedDate, QTime(23, 59, 59)), formHost);
    for (QDateTimeEdit* edit : {m_startTimeEdit, m_endTimeEdit}) {
        edit->setCalendarPopup(true);
        edit->setDisplayFormat("yyyy-MM-dd HH:mm");
        edit->setStyleSheet("font-size: 11px;");
    }
    addFieldRow(form, tr("Start:"), m_startTimeEdit);
    addFieldRow(form, tr("End:"), m_endTimeEdit);

    QPushButton* queryBtn = new QPushButton(tr("查询"), formHost);
    queryBtn->setToolTip(tr("按自定义时间范围查询"));
    connect(queryBtn, &QPushButton::clicked, this, &PlaybackChannel::onApplyTimeRange);
    form->addWidget(queryBtn);

    scroll->setWidget(formHost);
    return m_filterWidget;
}

QWidget* PlaybackChannel::buildFileListArea(QWidget* parent) {
    QFrame* frame = new QFrame(parent);
    frame->setStyleSheet("border: 1px solid #3e3e3e;");
    QVBoxLayout* layout = new QVBoxLayout(frame);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_statusLabel = new QLabel(tr("未加载"), frame);
    m_statusLabel->setStyleSheet("color: #9a9a9a; font-size: 11px; padding: 0 4px;");
    layout->addWidget(m_statusLabel);

    m_fileList = new RecordingFileListWidget(frame);
    m_fileList->setChannelKey(key());
    layout->addWidget(m_fileList, 1);

    connect(m_fileList, &RecordingFileListWidget::fileDoubleClicked, this,
            &PlaybackChannel::playFileRequested);
    connect(m_fileList, &RecordingFileListWidget::downloadRequested, this,
            &PlaybackChannel::onDownloadRequested);
    connect(m_fileList, &RecordingFileListWidget::pageSizeChanged, this, [this](int size) {
        m_pageSize = size;
        reload();
    });

    return frame;
}

QWidget* PlaybackChannel::buildBookmarkArea(QWidget* parent) {
    QWidget* box = new QWidget(parent);
    QVBoxLayout* layout = new QVBoxLayout(box);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(4);

    QHBoxLayout* header = new QHBoxLayout();
    header->setSpacing(4);
    QLabel* label = new QLabel(tr("Bookmarks"), box);
    label->setStyleSheet("color: #e0e0e0; font-weight: bold;");
    header->addWidget(label);
    header->addStretch(1);

    QPushButton* addBtn = new QPushButton(tr("添加书签"), box);
    addBtn->setFixedHeight(24);
    addBtn->setToolTip(tr("在当前播放位置打一个书签"));
    connect(addBtn, &QPushButton::clicked, this, [this]() {
        addBookmarkAt(m_playbackPositionMs);
    });
    header->addWidget(addBtn);

    QPushButton* delBtn = new QPushButton(tr("删除"), box);
    delBtn->setFixedHeight(24);
    connect(delBtn, &QPushButton::clicked, this, &PlaybackChannel::deleteSelectedBookmark);
    header->addWidget(delBtn);
    layout->addLayout(header);

    m_bookmarkList = new QListWidget(box);
    m_bookmarkList->setMaximumHeight(110);
    m_bookmarkList->setStyleSheet(R"(
        QListWidget {
            background-color: #1a1a1a;
            border: 1px solid #3e3e3e;
            color: #e0e0e0;
            font-size: 12px;
        }
        QListWidget::item:selected {
            background-color: #0078D4;
        }
    )");
    connect(m_bookmarkList, &QListWidget::itemDoubleClicked, this, [this](QListWidgetItem* item) {
        onBookmarkActivated(m_bookmarkList->row(item));
    });
    layout->addWidget(m_bookmarkList, 0);

    return box;
}

// ── 查询 ────────────────────────────────────────────────────────────

void PlaybackChannel::reload() {
    loadRecordings();
}

void PlaybackChannel::loadRecordings() {
    loadAllPages(++m_requestSeq, queryStartMs(), queryEndMs(), m_segmentType);
}

int64_t PlaybackChannel::queryStartMs() const {
    if (m_useCustomRange && m_startTimeEdit) {
        return m_startTimeEdit->dateTime().toMSecsSinceEpoch();
    }
    return QDateTime(m_selectedDate, QTime(0, 0, 0)).toMSecsSinceEpoch();
}

int64_t PlaybackChannel::queryEndMs() const {
    if (m_useCustomRange && m_endTimeEdit) {
        return m_endTimeEdit->dateTime().toMSecsSinceEpoch();
    }
    return QDateTime(m_selectedDate, QTime(23, 59, 59)).toMSecsSinceEpoch();
}

void PlaybackChannel::loadAllPages(quint64 seq, int64_t startMs, int64_t endMs,
                                   const QString& segmentType) {
    m_files.clear();
    // 旧通道的报警标记先清掉，避免查询期间显示错误的事件
    emit alarmsLoaded(QList<TimelineEvent>());
    if (m_statusLabel) {
        m_statusLabel->setText(tr("正在加载..."));
    }
    fetchPage(seq, startMs, endMs, segmentType, 1);
}

void PlaybackChannel::fetchPage(quint64 seq, int64_t startMs, int64_t endMs,
                                const QString& segmentType, int page) {
    QPointer<PlaybackChannel> self(this);
    if (m_statusLabel && page > 1) {
        m_statusLabel->setText(tr("正在加载第 %1 页...").arg(page));
    }
    RecordingApi::instance()->queryFiles(
        m_deviceTag,
        m_channelTag,
        startMs,
        endMs,
        segmentType,
        page,
        m_pageSize,
        [self, seq, startMs, endMs, segmentType, page](
            bool success, const RecordingFilePage& result, const QString& error) {
            if (!self) {
                return;   // 通道已销毁
            }
            if (seq != self->m_requestSeq) {
                return;   // 过期响应，直接丢弃
            }
            if (!success) {
                LOG_ERROR("PlaybackChannel", QString("queryFiles failed: %1").arg(error));
                self->m_statusLabel->setText(tr("加载失败"));
                return;
            }

            self->m_files.append(result.files);
            const bool more = result.hasMore
                && !result.files.isEmpty()
                && result.page < kMaxPages;
            if (more) {
                self->fetchPage(seq, startMs, endMs, segmentType, result.page + 1);
                return;
            }

            self->applyFiles(self->m_files);
            if (self->m_statusLabel) {
                self->m_statusLabel->setText(self->m_files.isEmpty()
                    ? tr("该时间段没有录像")
                    : tr("共 %1 段录像").arg(self->m_files.size()));
            }
            LOG_INFO("PlaybackChannel",
                QString("queryFiles: device=%1 channel=%2 files=%3")
                    .arg(self->m_deviceTag).arg(self->m_channelTag).arg(self->m_files.size()));

            emit self->filesLoaded(self->m_files);
            self->loadAlarms(startMs, endMs, seq);
        });
}

void PlaybackChannel::loadAlarms(int64_t startMs, int64_t endMs, quint64 seq) {
    QPointer<PlaybackChannel> self(this);
    RecordingApi::instance()->queryAlarms(
        m_deviceTag, startMs, endMs,
        [self, seq](bool success, const QList<AlarmEvent>& alarms, const QString& error) {
            if (!self) {
                return;
            }
            if (seq != self->m_requestSeq) {
                return;
            }
            if (!success) {
                LOG_WARNING("PlaybackChannel", QString("queryAlarms failed: %1").arg(error));
                return;
            }
            QList<TimelineEvent> events;
            events.reserve(alarms.size());
            for (const AlarmEvent& alarm : alarms) {
                TimelineEvent event;
                event.timeMs = alarm.timeMs;
                event.color = QColor("#ff6b6b");
                event.label = alarm.alarmType.isEmpty() ? tr("报警") : alarm.alarmType;
                events.append(event);
            }
            emit self->alarmsLoaded(events);
        });
}

void PlaybackChannel::applyFiles(const QList<RecordingFile>& files) {
    m_fileList->setFiles(files);
    updateTimelineRange();
}

void PlaybackChannel::updateTimelineRange() {
    // 时间轴范围 = 当前列出录像的总跨度
    int64_t start = 0;
    int64_t end = 0;
    for (const RecordingFile& file : m_files) {
        if (!file.timeValid) {
            continue;
        }
        if (start == 0 || file.startTimeMs < start) {
            start = file.startTimeMs;
        }
        if (file.endTimeMs > end) {
            end = file.endTimeMs;
        }
    }
    if (end <= start) {
        // 没有录像时回落到查询范围
        start = queryStartMs();
        end = queryEndMs();
    }
    m_rangeStartMs = start;
    m_rangeEndMs = end;
    if (m_timeline) {
        m_timeline->setTimeRange(start, end);
    }
    emit rangeChanged(start, end);
}

// ── 检索条件 ────────────────────────────────────────────────────────

void PlaybackChannel::onDateChanged(const QDate& date) {
    m_selectedDate = date;
    // 换日期后回到整天范围
    m_useCustomRange = false;
    if (m_startTimeEdit) {
        m_startTimeEdit->setDateTime(QDateTime(date, QTime(0, 0, 0)));
        m_endTimeEdit->setDateTime(QDateTime(date, QTime(23, 59, 59)));
    }
    reload();
}

void PlaybackChannel::onApplyTimeRange() {
    if (!m_startTimeEdit || !m_endTimeEdit) {
        return;
    }
    if (m_startTimeEdit->dateTime() >= m_endTimeEdit->dateTime()) {
        QMessageBox::warning(m_hostWidget.data(), tr("时间范围无效"), tr("开始时间必须早于结束时间。"));
        return;
    }
    m_useCustomRange = true;
    reload();
}

void PlaybackChannel::onSegmentTypeChanged(const QString& type) {
    if (m_segmentType == type) {
        return;
    }
    m_segmentType = type;
    reload();
}

// ── 书签 ────────────────────────────────────────────────────────────

void PlaybackChannel::refreshBookmarks() {
    if (!m_bookmarkList) {
        return;
    }
    m_bookmarkList->clear();
    m_visibleBookmarks.clear();

    // 只列当前通道的书签
    const QList<Bookmark> all = BookmarkStore::instance()->bookmarks();
    for (const Bookmark& b : all) {
        if (!m_deviceTag.isEmpty() && b.deviceTag != m_deviceTag) {
            continue;
        }
        if (!m_channelTag.isEmpty() && !b.channelTag.isEmpty() && b.channelTag != m_channelTag) {
            continue;
        }
        m_visibleBookmarks.append(b);
    }

    QList<TimelineEvent> marks;
    for (const Bookmark& b : m_visibleBookmarks) {
        const QString time = QDateTime::fromMSecsSinceEpoch(b.timeMs).toString("MM-dd hh:mm:ss");
        m_bookmarkList->addItem(b.note.isEmpty() ? time : QString("%1  %2").arg(time, b.note));
        TimelineEvent event;
        event.timeMs = b.timeMs;
        event.color = QColor("#ffd166");
        event.label = b.note;
        marks.append(event);
    }
    emit bookmarksChanged(marks);
}

void PlaybackChannel::addBookmarkAt(int64_t timeMs) {
    if (m_deviceTag.isEmpty() && m_channelTag.isEmpty()) {
        return;
    }
    if (timeMs <= 0) {
        timeMs = queryStartMs();
    }
    BookmarkStore::instance()->addBookmark(m_deviceTag, m_channelTag, timeMs);
    refreshBookmarks();
}

void PlaybackChannel::deleteSelectedBookmark() {
    if (!m_bookmarkList) {
        return;
    }
    const int row = m_bookmarkList->currentRow();
    if (row < 0 || row >= m_visibleBookmarks.size()) {
        return;
    }
    BookmarkStore::instance()->removeBookmark(m_visibleBookmarks[row].id);
    refreshBookmarks();
}

void PlaybackChannel::onBookmarkActivated(int row) {
    if (row < 0 || row >= m_visibleBookmarks.size()) {
        return;
    }
    emit seekRequested(m_visibleBookmarks[row].timeMs);
}

void PlaybackChannel::onDownloadRequested(const RecordingFile& file) {
    if (!m_downloader || file.url.isEmpty()) {
        return;
    }
    const QString suggested = QDir(QStandardPaths::writableLocation(QStandardPaths::DownloadLocation))
                                 .filePath(file.filename.isEmpty() ? QString("recording.mp4") : file.filename);
    const QString target = QFileDialog::getSaveFileName(m_hostWidget.data(), tr("保存录像文件"), suggested);
    if (target.isEmpty()) {
        return;
    }
    m_downloader->download(QUrl(file.url), target);
}

}
