#pragma once

#include "TourScheme.h"

#include <QWidget>

class QLabel;
class QPushButton;
class QTableView;

namespace rustcam {

class TourApi;
class TourChannelModel;
class TourPlayerManager;
class TourScheduler;
class TourSchemeListPanel;
class SingleInstanceGuard;

/// 运行页：左侧方案列表 + 右侧网格/状态表/控制条。
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
    void onStartRequested(const TourScheme& scheme);
    void onPauseClicked();
    void onStopClicked();
    void onNextBatchClicked();
    void onPrevBatchClicked();
    void onRetryClicked();
    void onTaskStateChanged(int state);
    void onBatchStarted(int batchIndex, int cycle, QVector<TourChannelRef> channels);
    void onWindowStateChanged(const QString& layoutItemId, TourChannelState state,
                              TourChannelRef channel, QString reason);
    void onCountdown(int dwellRemainMs, int batchTimeoutRemainMs);
    void onPageAtEnd();
    void onRetryRequested(const QString& layoutItemId);

private:
    void startScheme(const TourScheme& scheme);
    void updateButtons();
    void showLockedChannels(const TourScheme& scheme, const TourSnapshot& snapshot);

    TourApi* m_api = nullptr;
    TourSchemeListPanel* m_listPanel = nullptr;
    TourPlayerManager* m_players = nullptr;
    TourScheduler* m_scheduler = nullptr;
    TourChannelModel* m_model = nullptr;
    SingleInstanceGuard* m_guard = nullptr;

    QWidget* m_gridHost = nullptr;
    QTableView* m_tableView = nullptr;
    QLabel* m_schemeLabel = nullptr;
    QLabel* m_batchLabel = nullptr;
    QLabel* m_slotsLabel = nullptr;
    QLabel* m_remainLabel = nullptr;
    QLabel* m_stateLabel = nullptr;

    QPushButton* m_pauseButton = nullptr;
    QPushButton* m_stopButton = nullptr;
    QPushButton* m_prevButton = nullptr;
    QPushButton* m_nextButton = nullptr;
    QPushButton* m_retryButton = nullptr;

    TourScheme m_currentScheme;
};

} // namespace rustcam
