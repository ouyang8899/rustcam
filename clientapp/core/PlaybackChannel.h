#pragma once

#include <QWidget>
#include <QList>
#include <QDate>
#include <QPointer>
#include "CoreExport.h"
#include "RecordingTypes.h"
#include "TimelineWidget.h"
#include "BookmarkStore.h"

class QComboBox;
class QDateEdit;
class QDateTimeEdit;
class QLabel;
class QListWidget;
class QListWidgetItem;
class QPushButton;

namespace rustcam {

class RecordingFileListWidget;
class BookmarkListWidget;
class PlaybackGroup;
class ReplayPlayerContainer;

/// 一个通道 = 一个独立的回放会话
/// 拥有自己的检索控件、录像文件列表、书签列表、时间轴与播放位置
class RUSTCAM_CORE_API PlaybackChannel : public QObject {
    Q_OBJECT
public:
    PlaybackChannel(const QString& deviceTag,
                    const QString& channelTag,
                    const QString& displayName,
                    QObject* parent = nullptr);
    ~PlaybackChannel() override;

    QString key() const;
    QString deviceTag() const { return m_deviceTag; }
    QString channelTag() const { return m_channelTag; }
    QString displayName() const { return m_displayName; }

    /// 检索条件区（日期 / 类型 / 时间范围）
    QWidget* filterWidget() const { return m_filterWidget; }
    RecordingFileListWidget* fileList() const { return m_fileList; }
    BookmarkListWidget* bookmarkList() const { return m_bookmarkList; }
    /// 时间轴控件（显示在视频区下方，由 ReplayPlayerContainer 托管）
    TimelineWidget* timeline() const { return m_timeline.data(); }

    /// 侧栏页面：检索条件区 + 录像文件列表 + 书签列表
    QWidget* createSidePanelPage(QWidget* parent);
    /// 对话框的宿主控件（侧栏页面）
    void setHostWidget(QWidget* widget) { m_hostWidget = widget; }

    const QList<RecordingFile>& files() const { return m_files; }
    int64_t rangeStartMs() const { return m_rangeStartMs; }
    int64_t rangeEndMs() const { return m_rangeEndMs; }
    bool usesCustomRange() const { return m_useCustomRange; }
    QDate selectedDate() const { return m_selectedDate; }

    PlaybackGroup* group() const { return m_group; }
    void setGroup(PlaybackGroup* group) { m_group = group; }

    /// 播放器容器：时间轴上的定位直接调用容器 seek（目标唯一，无需再转一手）
    void setContainer(ReplayPlayerContainer* container) { m_container = container; }

    void setPlaybackPosition(int64_t timeMs) { m_playbackPositionMs = timeMs; }
    int64_t playbackPositionMs() const { return m_playbackPositionMs; }

public slots:
    void reload();
    /// 手动加载下一页（追加到列表并更新时间轴）
    void loadNextPage();
    void onDateChanged(const QDate& date);
    void onApplyTimeRange();
    void onSegmentTypeChanged(const QString& type);
    void addBookmarkAt(int64_t timeMs);
    void deleteSelectedBookmark();
    void onBookmarkActivated(int row);
    void refreshBookmarks();

signals:
    void filesLoaded(const QString& channelKey, const QList<RecordingFile>& files);
    void alarmsLoaded(const QString& channelKey, const QList<TimelineEvent>& events);
    void bookmarksChanged(const QString& channelKey, const QList<TimelineEvent>& bookmarks);
    void rangeChanged(const QString& channelKey, int64_t startMs, int64_t endMs);
    void playFileRequested(const QString& channelKey, const RecordingFile& file);
    void statusChanged(const QString& text);

private slots:
    void onTimeSelected(int64_t timeMs);
    void onPeriodClicked(const RecordingPeriod& period);
    void onPlayheadReleased(const QString& windowId, int64_t timeMs);
    void onTypeComboChanged(int index);
    void onPageSizeChanged(int size);
    void onBookmarkItemDoubleClicked(int row);
    void onAddBookmarkClicked();
    void onDeleteBookmarkClicked();
    void onFileDoubleClicked(const RecordingFile& file);
    void onDownloadFinished(bool success, const QString& path, const QString& error);

private:
    QWidget* buildFilterArea(QWidget* parent);
    QWidget* buildFileListArea(QWidget* parent);
    QWidget* buildBookmarkArea(QWidget* parent);

    void loadAllPages(quint64 seq, int64_t startMs, int64_t endMs, const QString& segmentType);
    void fetchPage(quint64 seq, int64_t startMs, int64_t endMs, const QString& segmentType, int page);
    void loadAlarms(int64_t startMs, int64_t endMs, quint64 seq);
    void updateTimelineRange();
    int64_t queryStartMs() const;
    int64_t queryEndMs() const;
    void onDownloadRequested(const RecordingFile& file);
    /// 定位到某个绝对时间：seek 本通道的播放组并同步播放位置
    void seekTo(int64_t timeMs);

private:
    QString m_key;
    QString m_deviceTag;
    QString m_channelTag;
    QString m_displayName;
    PlaybackGroup* m_group = nullptr;
    ReplayPlayerContainer* m_container = nullptr;

    QWidget* m_filterWidget = nullptr;
    QDateEdit* m_dateEdit = nullptr;
    QComboBox* m_typeCombo = nullptr;
    QDateTimeEdit* m_startTimeEdit = nullptr;
    QDateTimeEdit* m_endTimeEdit = nullptr;
    RecordingFileListWidget* m_fileList = nullptr;
    QLabel* m_statusLabel = nullptr;
    /// 加载更多（还有下一页时显示）
    QPushButton* m_loadMoreBtn = nullptr;
    BookmarkListWidget* m_bookmarkList = nullptr;
    QPointer<TimelineWidget> m_timeline;
    /// 对话框宿主（侧栏页面）
    QPointer<QWidget> m_hostWidget;
    class FileDownloader* m_downloader = nullptr;

    QList<RecordingFile> m_files;
    QDate m_selectedDate;
    QString m_segmentType;
    bool m_useCustomRange = false;
    int64_t m_rangeStartMs = 0;
    int64_t m_rangeEndMs = 0;
    int64_t m_playbackPositionMs = 0;
    /// 每次查询的条数（服务端上限 500，条数多时少发几次请求）
    int m_pageSize = 200;
    /// 已加载到第几页
    int m_loadedPage = 0;
    /// 服务端返回的录像总条数
    int m_total = 0;
    /// 还有下一页
    bool m_hasMore = false;
    /// 正在请求中
    bool m_loading = false;
    quint64 m_requestSeq = 0;
};

}
