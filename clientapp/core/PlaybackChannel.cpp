#include "PlaybackChannel.h"
#include "RecordingApi.h"
#include "FileDownloader.h"
#include "RecordingFileListWidget.h"
#include "BookmarkListWidget.h"
#include "ReplayPlayerContainer.h"
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

/// 检索条件区域固定高度（超出部分在滚动区里滚动）
constexpr int kFilterAreaHeight = 240;

/// 表单里的一行：标签在上、控件在下
void addFieldRow(QVBoxLayout* form, const QString& text, QWidget* field)
{
    QLabel* label = new QLabel(text, field);
    label->setObjectName("PlaybackFilterFieldLabel");
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
    , m_key(deviceTag + "/" + channelTag)
    , m_deviceTag(deviceTag)
    , m_channelTag(channelTag)
    , m_displayName(displayName)
    , m_selectedDate(QDate::currentDate())
{
    // 时间轴由 ReplayPlayerContainer 托管显示（视频区下方）
    m_timeline = new TimelineWidget();
    connect(m_timeline, &TimelineWidget::timeSelected, this, &PlaybackChannel::onTimeSelected);
    connect(m_timeline, &TimelineWidget::periodClicked, this, &PlaybackChannel::onPeriodClicked);
    connect(m_timeline, &TimelineWidget::playheadReleased, this, &PlaybackChannel::onPlayheadReleased);

    m_downloader = new FileDownloader(this);
    connect(m_downloader, &FileDownloader::finished, this, &PlaybackChannel::onDownloadFinished);

    // 书签存储是全局单例：任何通道新增/删除书签，所有通道各自刷新自己的列表
    connect(BookmarkStore::instance(), &BookmarkStore::bookmarksChanged,
            this, &PlaybackChannel::refreshBookmarks);
}

PlaybackChannel::~PlaybackChannel() {
}

QString PlaybackChannel::key() const {
    return m_key;
}

// ── 侧栏页面 ────────────────────────────────────────────────────────

QWidget* PlaybackChannel::createSidePanelPage(QWidget* parent) {
    QWidget* page = new QWidget(parent);
    m_hostWidget = page;
    QVBoxLayout* layout = new QVBoxLayout(page);
    layout->setContentsMargins(4, 4, 4, 4);
    layout->setSpacing(4);

    QLabel* filterTitle = new QLabel(tr("检索条件"), page);
    filterTitle->setObjectName("PlaybackSectionTitle");
    layout->addWidget(filterTitle);

    QFrame* filterFrame = new QFrame(page);
    filterFrame->setObjectName("PlaybackFilterFrame");
    QVBoxLayout* filterFrameLayout = new QVBoxLayout(filterFrame);
    filterFrameLayout->setContentsMargins(2, 2, 2, 2);
    filterFrameLayout->setSpacing(0);
    filterFrameLayout->addWidget(buildFilterArea(page));
    layout->addWidget(filterFrame, 0);

    QLabel* fileListLabel = new QLabel(tr("Recording Files"), page);
    fileListLabel->setObjectName("PlaybackSectionTitle");
    layout->addWidget(fileListLabel);

    layout->addWidget(buildFileListArea(page), 1);
    layout->addWidget(buildBookmarkArea(page), 0);

    // 书签列表初次显示已有书签
    refreshBookmarks();

    return page;
}

QWidget* PlaybackChannel::buildFilterArea(QWidget* parent) {
    m_filterWidget = new QWidget(parent);
    QVBoxLayout* outer = new QVBoxLayout(m_filterWidget);
    outer->setContentsMargins(0, 0, 0, 0);
    outer->setSpacing(0);

    QScrollArea* scroll = new QScrollArea(m_filterWidget);
    scroll->setObjectName("PlaybackFilterScroll");
    scroll->setWidgetResizable(true);
    scroll->setFrameShape(QFrame::NoFrame);
    scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    scroll->setFixedHeight(kFilterAreaHeight);
    outer->addWidget(scroll);

    QWidget* formHost = new QWidget(scroll);
    QVBoxLayout* form = new QVBoxLayout(formHost);
    form->setContentsMargins(4, 4, 4, 4);
    form->setSpacing(4);

    m_dateEdit = new QDateEdit(m_selectedDate, formHost);
    m_dateEdit->setObjectName("PlaybackFilterInput");
    m_dateEdit->setCalendarPopup(true);
    connect(m_dateEdit, &QDateEdit::dateChanged, this, &PlaybackChannel::onDateChanged);
    addFieldRow(form, tr("Date:"), m_dateEdit);

    m_typeCombo = new QComboBox(formHost);
    m_typeCombo->addItem(tr("全部类型"), QString());
    m_typeCombo->addItem(tr("普通"), QStringLiteral("normal"));
    m_typeCombo->addItem(tr("报警"), QStringLiteral("alarm"));
    m_typeCombo->addItem(tr("预录"), QStringLiteral("prebuffer"));
    m_typeCombo->setToolTip(tr("按录像分段类型过滤"));
    connect(m_typeCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &PlaybackChannel::onTypeComboChanged);
    addFieldRow(form, tr("Type:"), m_typeCombo);

    m_startTimeEdit = new QDateTimeEdit(QDateTime(m_selectedDate, QTime(0, 0, 0)), formHost);
    m_endTimeEdit = new QDateTimeEdit(QDateTime(m_selectedDate, QTime(23, 59, 59)), formHost);
    for (QDateTimeEdit* edit : {m_startTimeEdit, m_endTimeEdit}) {
        edit->setObjectName("PlaybackFilterInput");
        edit->setCalendarPopup(true);
        edit->setDisplayFormat("yyyy-MM-dd HH:mm");
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
    frame->setObjectName("PlaybackFileListFrame");
    QVBoxLayout* layout = new QVBoxLayout(frame);
    layout->setContentsMargins(2, 2, 2, 2);
    layout->setSpacing(0);

    m_statusLabel = new QLabel(tr("未加载"), frame);
    m_statusLabel->setObjectName("PlaybackStatusLabel");
    layout->addWidget(m_statusLabel);

    m_fileList = new RecordingFileListWidget(frame);
    m_fileList->setChannelKey(key());
    layout->addWidget(m_fileList, 1);

    m_loadMoreBtn = new QPushButton(tr("加载更多"), frame);
    m_loadMoreBtn->setObjectName("LoadMoreButton");
    m_loadMoreBtn->hide();
    connect(m_loadMoreBtn, &QPushButton::clicked, this, &PlaybackChannel::loadNextPage);
    layout->addWidget(m_loadMoreBtn, 0);

    connect(m_fileList, &RecordingFileListWidget::fileDoubleClicked, this,
            &PlaybackChannel::onFileDoubleClicked);
    connect(m_fileList, &RecordingFileListWidget::downloadRequested, this,
            &PlaybackChannel::onDownloadRequested);
    connect(m_fileList, &RecordingFileListWidget::pageSizeChanged,
            this, &PlaybackChannel::onPageSizeChanged);

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
    label->setObjectName("PlaybackSectionTitle");
    header->addWidget(label);
    header->addStretch(1);

    QPushButton* addBtn = new QPushButton(tr("添加书签"), box);
    addBtn->setFixedHeight(24);
    addBtn->setToolTip(tr("在当前播放位置打一个书签"));
    connect(addBtn, &QPushButton::clicked, this, &PlaybackChannel::onAddBookmarkClicked);
    header->addWidget(addBtn);

    QPushButton* delBtn = new QPushButton(tr("删除"), box);
    delBtn->setFixedHeight(24);
    connect(delBtn, &QPushButton::clicked, this, &PlaybackChannel::onDeleteBookmarkClicked);
    header->addWidget(delBtn);
    layout->addLayout(header);

    m_bookmarkList = new BookmarkListWidget(box);
    layout->addWidget(m_bookmarkList, 0);

    // 双击用行号标识，不传 item 指针（列表刷新后指针可能已失效）
    connect(m_bookmarkList, &BookmarkListWidget::bookmarkActivated,
            this, &PlaybackChannel::onBookmarkItemDoubleClicked);

    return box;
}

// ── 查询 ────────────────────────────────────────────────────────────

void PlaybackChannel::reload() {
    // 重新查询：作废在途请求，清空已加载的，重新从第一页开始
    ++m_requestSeq;
    m_loading = false;
    m_files.clear();
    m_loadedPage = 0;
    m_total = 0;
    m_hasMore = false;
    m_fileList->clearFiles();
    if (m_loadMoreBtn) {
        m_loadMoreBtn->hide();
        m_loadMoreBtn->setEnabled(true);
    }
    loadNextPage();
}

void PlaybackChannel::loadNextPage() {
    if (m_loading) {
        return;   // 正在加载
    }
    m_loading = true;
    if (m_loadMoreBtn) {
        m_loadMoreBtn->setEnabled(false);
    }
    // 同一轮查询内翻页共用 seq，避免加载更多时把首页的报警请求判为过期
    loadAllPages(m_requestSeq, queryStartMs(), queryEndMs(), m_segmentType);
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
    fetchPage(seq, startMs, endMs, segmentType, m_loadedPage + 1);
}

void PlaybackChannel::fetchPage(quint64 seq, int64_t startMs, int64_t endMs,
                                const QString& segmentType, int page) {
    QPointer<PlaybackChannel> self(this);
    if (page == 1) {
        // 新查询先清掉旧报警标记，避免查询期间显示错位事件；
        // 加载更多时不能清，否则翻页后报警会消失
        emit alarmsLoaded(m_key, QList<TimelineEvent>());
        if (m_statusLabel) {
            m_statusLabel->setText(tr("正在加载..."));
        }
    } else if (m_statusLabel) {
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
                self->m_loading = false;
                if (self->m_statusLabel) {
                    self->m_statusLabel->setText(tr("加载失败"));
                    self->m_statusLabel->setToolTip(error);
                }
                if (self->m_loadMoreBtn) {
                    self->m_loadMoreBtn->setEnabled(true);
                    self->m_loadMoreBtn->setVisible(true);
                    self->m_loadMoreBtn->setText(tr("加载失败，点击重试"));
                }
                return;
            }

            self->m_files.append(result.files);
            self->m_loadedPage = result.page > 0 ? result.page : page;
            self->m_total = result.total;
            self->m_hasMore = result.hasMore && !result.files.isEmpty();
            self->m_loading = false;

            // 追加到列表（不清空已加载的），并同步时间轴范围
            for (const RecordingFile& file : result.files) {
                self->m_fileList->addFile(file);
            }
            // addFile 只知道已加载条数，这里补上服务端总数
            self->m_fileList->setPagingInfo(self->m_total, self->m_files.size());
            self->updateTimelineRange();

            if (self->m_statusLabel) {
                self->m_statusLabel->setText(tr("共 %1 段录像（已加载 %2 段）")
                    .arg(self->m_total).arg(self->m_files.size()));
                self->m_statusLabel->setToolTip(QString());
            }
            if (self->m_loadMoreBtn) {
                self->m_loadMoreBtn->setEnabled(true);
                self->m_loadMoreBtn->setVisible(self->m_hasMore);
                self->m_loadMoreBtn->setText(tr("加载更多（剩余 %1 段）")
                    .arg(qMax(0, self->m_total - self->m_files.size())));
            }
            LOG_INFO("PlaybackChannel",
                QString("queryFiles: device=%1 channel=%2 page=%3 loaded=%4 total=%5 hasMore=%6")
                    .arg(self->m_deviceTag).arg(self->m_channelTag).arg(self->m_loadedPage)
                    .arg(self->m_files.size()).arg(self->m_total)
                    .arg(self->m_hasMore ? "yes" : "no"));

            // 交给外部：列表/时间轴分段按已加载的全部数据更新
            emit self->filesLoaded(self->m_key, self->m_files);
            if (page == 1) {
                self->loadAlarms(startMs, endMs, seq);
            }
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
            emit self->alarmsLoaded(self->m_key, events);
        });
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
    emit rangeChanged(m_key, start, end);
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

    // 只列当前通道的书签
    QList<Bookmark> visible;
    const QList<Bookmark> all = BookmarkStore::instance()->bookmarks();
    for (const Bookmark& b : all) {
        if (!m_deviceTag.isEmpty() && b.deviceTag != m_deviceTag) {
            continue;
        }
        if (!m_channelTag.isEmpty() && !b.channelTag.isEmpty() && b.channelTag != m_channelTag) {
            continue;
        }
        visible.append(b);
    }
    m_bookmarkList->setBookmarks(visible);

    QList<TimelineEvent> marks;
    for (const Bookmark& b : visible) {
        TimelineEvent event;
        event.timeMs = b.timeMs;
        event.color = QColor("#ffd166");
        event.label = b.note;
        marks.append(event);
    }
    emit bookmarksChanged(m_key, marks);
}

void PlaybackChannel::addBookmarkAt(int64_t timeMs) {
    if (m_deviceTag.isEmpty() && m_channelTag.isEmpty()) {
        return;
    }
    if (timeMs <= 0) {
        timeMs = queryStartMs();
    }
    BookmarkStore::instance()->addBookmark(m_deviceTag, m_channelTag, timeMs);
    // 列表刷新由 BookmarkStore::bookmarksChanged 触发
}

void PlaybackChannel::deleteSelectedBookmark() {
    Bookmark bookmark;
    if (!m_bookmarkList || !m_bookmarkList->currentBookmark(&bookmark)) {
        return;
    }
    BookmarkStore::instance()->removeBookmark(bookmark.id);
}

void PlaybackChannel::onBookmarkActivated(int row) {
    Bookmark bookmark;
    if (!m_bookmarkList || !m_bookmarkList->model()->bookmarkAt(row, &bookmark)) {
        return;
    }
    seekTo(bookmark.timeMs);
}

// ── 定位（时间轴 / 书签）────────────────────────────────────────────

void PlaybackChannel::onTimeSelected(int64_t timeMs) {
    seekTo(timeMs);
}

void PlaybackChannel::onPeriodClicked(const RecordingPeriod& period) {
    seekTo(period.startTimeMs);
}

void PlaybackChannel::onPlayheadReleased(const QString& windowId, int64_t timeMs) {
    Q_UNUSED(windowId)
    seekTo(timeMs);
}

/// 定位到某个绝对时间：目标唯一，直接调用播放器容器
void PlaybackChannel::seekTo(int64_t timeMs) {
    if (m_container) {
        m_container->seekChannel(m_key, timeMs);
    }
    setPlaybackPosition(timeMs);
}

// ── 检索条件回调 ────────────────────────────────────────────────────

void PlaybackChannel::onTypeComboChanged(int index) {
    Q_UNUSED(index)
    onSegmentTypeChanged(m_typeCombo->currentData().toString());
}

void PlaybackChannel::onPageSizeChanged(int size) {
    m_pageSize = size;
    reload();
}

// ── 书签回调 ────────────────────────────────────────────────────────

void PlaybackChannel::onBookmarkItemDoubleClicked(int row) {
    onBookmarkActivated(row);
}

void PlaybackChannel::onAddBookmarkClicked() {
    addBookmarkAt(m_playbackPositionMs);
}

void PlaybackChannel::onDeleteBookmarkClicked() {
    deleteSelectedBookmark();
}

// ── 录像文件 ────────────────────────────────────────────────────────

void PlaybackChannel::onFileDoubleClicked(const RecordingFile& file) {
    emit playFileRequested(m_key, file);
}

void PlaybackChannel::onDownloadFinished(bool success, const QString& path, const QString& error) {
    if (success) {
        LOG_INFO("PlaybackChannel", QString("download finished: %1").arg(path));
        QMessageBox::information(m_hostWidget.data(), tr("下载完成"),
            tr("录像文件已保存到：%1").arg(path));
    } else {
        LOG_ERROR("PlaybackChannel", QString("download failed: %1").arg(error));
        QMessageBox::warning(m_hostWidget.data(), tr("下载失败"), error);
    }
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
