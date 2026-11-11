#include "TourRunWidget.h"

#include <QHBoxLayout>
#include <QHeaderView>
#include <QLabel>
#include <QMessageBox>
#include <QPushButton>
#include <QSplitter>
#include <QVBoxLayout>

#include "SingleInstanceGuard.h"
#include "TourApi.h"
#include "TourAuditLog.h"
#include "TourChannelModel.h"
#include "TourChannelStatusPanel.h"
#include "TourPlayerManager.h"
#include "TourScheduler.h"
#include "TourSchemeListPanel.h"

namespace rustcam {

TourRunWidget::TourRunWidget(TourApi* api, QWidget* parent)
    : QWidget(parent)
    , m_api(api)
{
    QSplitter* splitter = new QSplitter(Qt::Horizontal, this);

    // 左栏：方案树（上）与通道状态列表（下），两者可上下拖动调整高度。
    QSplitter* leftSplitter = new QSplitter(Qt::Vertical, splitter);

    m_listPanel = new TourSchemeListPanel(m_api, leftSplitter);
    m_listPanel->setMinimumWidth(280);
    m_listPanel->setMaximumWidth(360);
    leftSplitter->addWidget(m_listPanel);

    m_statusPanel = new TourChannelStatusPanel(leftSplitter);
    leftSplitter->addWidget(m_statusPanel);
    leftSplitter->setStretchFactor(0, 1);
    leftSplitter->setStretchFactor(1, 1);

    splitter->addWidget(leftSplitter);

    QWidget* right = new QWidget(splitter);
    QVBoxLayout* rightLayout = new QVBoxLayout(right);
    rightLayout->setContentsMargins(0, 0, 0, 0);

    QHBoxLayout* topBar = new QHBoxLayout();
    m_schemeLabel = new QLabel(QStringLiteral("-"), right);
    m_batchLabel = new QLabel(QStringLiteral("-"), right);
    m_slotsLabel = new QLabel(QStringLiteral("-"), right);
    m_remainLabel = new QLabel(QStringLiteral("-"), right);
    m_stateLabel = new QLabel(QStringLiteral("-"), right);
    topBar->addWidget(m_schemeLabel);
    topBar->addWidget(m_batchLabel);
    topBar->addWidget(m_slotsLabel);
    topBar->addWidget(m_remainLabel);
    topBar->addWidget(m_stateLabel);
    topBar->addStretch(1);
    rightLayout->addLayout(topBar);

    m_gridHost = new QWidget(right);
    m_gridHost->setObjectName(QStringLiteral("TourGridHost"));
    m_gridHost->setMinimumHeight(320);
    rightLayout->addWidget(m_gridHost, 1);

    QHBoxLayout* controls = new QHBoxLayout();
    m_pauseButton = new QPushButton(tr("暂停"), right);
    m_stopButton = new QPushButton(tr("停止"), right);
    m_prevButton = new QPushButton(tr("上一批"), right);
    m_nextButton = new QPushButton(tr("下一批"), right);
    controls->addWidget(m_pauseButton);
    controls->addWidget(m_stopButton);
    controls->addWidget(m_prevButton);
    controls->addWidget(m_nextButton);
    controls->addStretch(1);
    rightLayout->addLayout(controls);

    splitter->addWidget(right);
    splitter->setStretchFactor(1, 1);
    QVBoxLayout* rootLayout = new QVBoxLayout(this);
    rootLayout->setContentsMargins(0, 0, 0, 0);
    rootLayout->addWidget(splitter);

    m_scheduler = new TourScheduler(this);
    m_players = new TourPlayerManager(m_api, m_gridHost, this);
    m_model = m_statusPanel->model();

    connect(m_listPanel, &TourSchemeListPanel::startRequested, this, &TourRunWidget::onStartRequested);
    connect(m_listPanel, &TourSchemeListPanel::schemeSelected, this, &TourRunWidget::onSchemeSelected);
    connect(m_listPanel, &TourSchemeListPanel::notify, this, [this](const QString& message, bool isError) {
        emit notify(message, isError);
    });

    connect(m_statusPanel, &TourChannelStatusPanel::retryRequested, this, &TourRunWidget::onRetryRequested);

    connect(m_scheduler, &TourScheduler::playRequested, m_players, &TourPlayerManager::play);
    connect(m_scheduler, &TourScheduler::releaseRequested, this, [this](const QString& layoutItemId) {
        if (layoutItemId == QStringLiteral("*")) {
            m_players->releaseAll();
        } else {
            m_players->releaseSlot(layoutItemId);
        }
    });
    connect(m_players, &TourPlayerManager::firstFrame, m_scheduler, &TourScheduler::onFirstFrame);
    connect(m_players, &TourPlayerManager::playFailed, m_scheduler, &TourScheduler::onPlayFailed);
    connect(m_players, &TourPlayerManager::disconnected, m_scheduler, &TourScheduler::onDisconnected);
    connect(m_players, &TourPlayerManager::stalled, m_scheduler, &TourScheduler::onStalled);
    connect(m_players, &TourPlayerManager::fullscreenChanged, this,
            [this](const QString&, bool fullscreen) { m_scheduler->setFullscreenPaused(fullscreen); });
    connect(m_players, &TourPlayerManager::retryRequested, this, &TourRunWidget::onCellRetryRequested);

    connect(m_scheduler, &TourScheduler::windowStateChanged, this,
            &TourRunWidget::onWindowStateChanged);
    connect(m_scheduler, &TourScheduler::batchStarted, this, &TourRunWidget::onBatchStarted);
    connect(m_scheduler, &TourScheduler::taskStateChanged, this, &TourRunWidget::onTaskStateChanged);
    connect(m_scheduler, &TourScheduler::countdown, this, &TourRunWidget::onCountdown);
    connect(m_scheduler, &TourScheduler::pageAtEnd, this, &TourRunWidget::onPageAtEnd);

    connect(m_pauseButton, &QPushButton::clicked, this, &TourRunWidget::onPauseClicked);
    connect(m_stopButton, &QPushButton::clicked, this, &TourRunWidget::onStopClicked);
    connect(m_nextButton, &QPushButton::clicked, this, &TourRunWidget::onNextBatchClicked);
    connect(m_prevButton, &QPushButton::clicked, this, &TourRunWidget::onPrevBatchClicked);

    updateButtons();
}

TourRunWidget::~TourRunWidget()
{
    stopTask();
}

void TourRunWidget::onSessionReady()
{
    if (!m_api) {
        return;
    }
    m_api->loadLayouts([this](bool success, const QList<PlayerLayout>& layouts, const QString& error) {
        if (!success) {
            emit notify(tr("加载布局失败：%1").arg(error), true);
            return;
        }
        m_layouts = layouts;
        m_listPanel->setLayouts(layouts);
        refreshGrid();
    });
    m_listPanel->refresh();
}

void TourRunWidget::stopTask()
{
    if (m_scheduler) {
        m_scheduler->stop();
    }
    if (m_players) {
        m_players->releaseAll();
    }
    delete m_guard;
    m_guard = nullptr;
}

const PlayerLayout* TourRunWidget::layoutById(int layoutId) const
{
    for (const PlayerLayout& layout : m_layouts) {
        if (layout.id == layoutId) {
            return &layout;
        }
    }
    return nullptr;
}

void TourRunWidget::refreshGrid()
{
    if (!m_players) {
        return;
    }
    // 轮询运行中不改网格：重建会释放正在播放的窗口。
    if (m_scheduler) {
        const TourTaskState state = m_scheduler->taskState();
        if (state == TourTaskState::Running || state == TourTaskState::Paused) {
            return;
        }
    }
    // 选中方案的布局优先；未选中时用默认布局，都没有则用第一个布局。
    TourScheme scheme;
    const PlayerLayout* target = nullptr;
    if (m_listPanel && m_listPanel->currentScheme(&scheme)) {
        target = layoutById(scheme.layoutId);
    }
    if (!target) {
        for (const PlayerLayout& layout : m_layouts) {
            if (layout.isDefault) {
                target = &layout;
                break;
            }
        }
    }
    if (!target && !m_layouts.isEmpty()) {
        target = &m_layouts.first();
    }
    if (target) {
        m_players->setLayout(*target);
    }
}

void TourRunWidget::onStartRequested(const TourScheme& scheme)
{
    startScheme(scheme);
}

void TourRunWidget::onSchemeSelected(const TourScheme& scheme)
{
    Q_UNUSED(scheme)
    // 选中方案即刷新右侧网格：未开始轮询时也能看到该方案的窗口布局。
    refreshGrid();
}

void TourRunWidget::startScheme(const TourScheme& scheme)
{
    if (!m_api) {
        return;
    }
    m_currentScheme = scheme;

    m_api->loadLayouts([this, scheme](bool success, const QList<PlayerLayout>& layouts, const QString& error) {
        if (!success) {
            emit notify(tr("加载布局失败：%1").arg(error), true);
            return;
        }
        m_layouts = layouts;
        m_listPanel->setLayouts(layouts);
        const PlayerLayout* target = layoutById(scheme.layoutId);
        if (!target) {
            emit notify(tr("布局 %1 不存在").arg(scheme.layoutId), true);
            return;
        }

        QStringList layoutItemIds;
        for (const LayoutItem& item : target->layoutJson) {
            layoutItemIds.append(item.id);
        }
        const TourSnapshot snapshot = TourSchemeCodec::buildSnapshot(scheme, layoutItemIds.size(), layoutItemIds);
        if (snapshot.activeQueue.isEmpty()) {
            emit notify(tr("方案没有可轮询的通道"), true);
            m_scheduler->start(scheme, snapshot, QStringList());
            return;
        }
        if (snapshot.layoutW - snapshot.lockedSet.size() <= 0) {
            emit notify(tr("全锁不可轮询"), true);
            m_scheduler->start(scheme, snapshot, QStringList());
            return;
        }

        delete m_guard;
        m_guard = new SingleInstanceGuard(scheme.id);
        if (!m_guard->tryLock()) {
            emit notify(tr("该方案已在另一实例运行"), true);
            return;
        }

        m_players->setLayout(*target);
        m_model->reset(scheme.channels);
        showLockedChannels(scheme, snapshot);
        m_scheduler->start(scheme, snapshot, QStringList());

        m_schemeLabel->setText(scheme.name);
        m_slotsLabel->setText(tr("可用槽位 %1").arg(snapshot.layoutW - snapshot.lockedSet.size()));

        QJsonObject detail;
        detail.insert(QStringLiteral("scheme"), scheme.id);
        detail.insert(QStringLiteral("name"), scheme.name);
        TourAuditLog::log(QStringLiteral("task_start"), detail);
    });
}

void TourRunWidget::showLockedChannels(const TourScheme& scheme, const TourSnapshot& snapshot)
{
    Q_UNUSED(scheme)
    for (const LockedSlot& slot : snapshot.lockedSet) {
        TourChannelRef ref;
        ref.deviceTag = slot.deviceTag;
        ref.channelTag = slot.channelTag;
        // generation 0 表示该窗口不受调度器管理（常驻锁定）。
        m_players->play(slot.layoutItemId, ref, 0);
    }
}

void TourRunWidget::onPauseClicked()
{
    if (!m_scheduler) {
        return;
    }
    if (m_scheduler->taskState() == TourTaskState::Paused) {
        m_scheduler->resume();
    } else {
        m_scheduler->pause();
    }
}

void TourRunWidget::onStopClicked()
{
    stopTask();
}

void TourRunWidget::onNextBatchClicked()
{
    if (m_scheduler) {
        m_scheduler->nextBatch();
    }
}

void TourRunWidget::onPrevBatchClicked()
{
    if (m_scheduler) {
        m_scheduler->prevBatch();
    }
}

void TourRunWidget::onRetryRequested(int row)
{
    if (!m_scheduler) {
        return;
    }
    if (row < 0) {
        emit notify(tr("请先在状态表中选择通道"), false);
        return;
    }
    if (row >= m_currentScheme.channels.size()) {
        return;
    }
    const TourChannelRef ref = m_currentScheme.channels.at(row).ref();
    const QString layoutItemId = m_scheduler->layoutItemIdForChannel(ref);
    if (layoutItemId.isEmpty()) {
        emit notify(tr("该通道不在当前批次中"), false);
        return;
    }
    m_scheduler->retrySlot(layoutItemId);
}

void TourRunWidget::onCellRetryRequested(const QString& layoutItemId)
{
    if (m_scheduler) {
        m_scheduler->retrySlot(layoutItemId);
    }
}

void TourRunWidget::onTaskStateChanged(int state)
{
    m_stateLabel->setText(tourTaskStateText(static_cast<TourTaskState>(state)));
    updateButtons();
}

void TourRunWidget::onBatchStarted(int batchIndex, int cycle, QVector<TourChannelRef> channels)
{
    m_batchLabel->setText(tr("第 %1 次 · 第 %2 批 · %3 路").arg(cycle + 1).arg(batchIndex + 1).arg(channels.size()));
    m_model->setCurrentBatch(batchIndex);
}

void TourRunWidget::onWindowStateChanged(const QString& layoutItemId, TourChannelState state,
                                         TourChannelRef channel, QString reason)
{
    if (!channel.isEmpty()) {
        m_model->setState(channel, state, reason, m_scheduler->currentBatchIndex());
    }
    // 窗口角标跟随调度器状态：已跳过/已锁定等只有调度器知道。
    if (m_players) {
        m_players->setSlotState(layoutItemId, state, reason);
    }
}

void TourRunWidget::onCountdown(int dwellRemainMs, int batchTimeoutRemainMs)
{
    const int remain = dwellRemainMs > 0 ? dwellRemainMs : batchTimeoutRemainMs;
    m_remainLabel->setText(tr("本批剩余 %1s").arg(remain / 1000));
}

void TourRunWidget::onPageAtEnd()
{
    emit notify(tr("已到队尾"), false);
}

void TourRunWidget::updateButtons()
{
    const TourTaskState state = m_scheduler ? m_scheduler->taskState() : TourTaskState::Idle;
    const bool running = state == TourTaskState::Running || state == TourTaskState::Paused;
    m_pauseButton->setEnabled(running);
    m_pauseButton->setText(state == TourTaskState::Paused ? tr("恢复") : tr("暂停"));
    m_stopButton->setEnabled(running);
    m_prevButton->setEnabled(running);
    m_nextButton->setEnabled(running);
    m_statusPanel->setRetryEnabled(running);
}

} // namespace rustcam
