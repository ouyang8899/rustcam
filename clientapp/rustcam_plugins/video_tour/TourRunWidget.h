#pragma once

#include "PlayerLayout.h"
#include "TourScheme.h"

#include <QHash>
#include <QList>
#include <QWidget>

class QLabel;
class QPushButton;

namespace rustcam {

class TourApi;
class TourChannelModel;
class TourChannelStatusPanel;
class TourPlayerManager;
class TourScheduler;
class TourSchemeListPanel;
class SingleInstanceGuard;

/// 运行页：左侧方案区（方案树 + 通道状态列表）+ 右侧视频网格/控制条。
class TourRunWidget : public QWidget {
    Q_OBJECT
public:
    explicit TourRunWidget(TourApi* api, QWidget* parent = nullptr);
    ~TourRunWidget() override;

    /// 登录后刷新方案与布局。
    void onSessionReady();
    /// 停止当前任务并释放播放器。
    void stopTask();

signals:
    void notify(const QString& message, bool isError);

private slots:
    void onStartClicked();
    void onSchemeSelected(const TourScheme& scheme);
    void onPauseClicked();
    void onStopClicked();
    void onNextBatchClicked();
    void onPrevBatchClicked();
    void onRetryClicked();
    void onTaskStateChanged(int state);
    void onBatchStarted(int batchIndex, int cycle, QVector<TourChannelRef> channels);
    void onWindowStateChanged(const QString& layoutItemId, TourChannelState state,
                              TourChannelRef channel, QString reason);
    void onCountdown(int dwellRemainMs, int batchTimeoutMs);
    void onPageAtEnd();
    void onCellRetryRequested(const QString& layoutItemId);

private:
    void startScheme(const TourScheme& scheme);
    void updateButtons();
    /// 方案树里是否有选中项。
    bool hasSelectedScheme() const;
    /// 轮询任务是否正在进行或暂停。
    bool isTaskActive() const;
    void showLockedChannels(const TourScheme& scheme, const TourSnapshot& snapshot);
    /// 锁定窗口不受调度器管理，播放事件需自行回写状态。
    void updateLockedChannelState(const QString& layoutItemId, quint64 generation,
                                  TourChannelState state, const QString& reason);
    /// 按当前选中方案（或默认布局）重建视频网格。
    void refreshGrid();
    /// 按布局 id 取布局；找不到返回 nullptr。
    const PlayerLayout* layoutById(int layoutId) const;

    TourApi* m_api = nullptr;
    TourSchemeListPanel* m_listPanel = nullptr;
    TourChannelStatusPanel* m_statusPanel = nullptr;
    TourPlayerManager* m_players = nullptr;
    TourScheduler* m_scheduler = nullptr;
    TourChannelModel* m_model = nullptr;
    SingleInstanceGuard* m_guard = nullptr;

    QList<PlayerLayout> m_layouts;
    /// 常驻锁定窗口：layoutItemId -> 通道。
    QHash<QString, TourChannelRef> m_lockedChannels;

    QWidget* m_gridHost = nullptr;
    QLabel* m_schemeLabel = nullptr;
    QLabel* m_batchLabel = nullptr;
    QLabel* m_slotsLabel = nullptr;
    QLabel* m_remainLabel = nullptr;
    QLabel* m_stateLabel = nullptr;

    QPushButton* m_startButton = nullptr;
    QPushButton* m_retryButton = nullptr;
    QPushButton* m_pauseButton = nullptr;
    QPushButton* m_stopButton = nullptr;
    QPushButton* m_prevButton = nullptr;
    QPushButton* m_nextButton = nullptr;

    TourScheme m_currentScheme;
};

} // namespace rustcam
