#include "TourPlayerManager.h"

#include <QGridLayout>
#include <QLabel>
#include <QTimer>
#include <QToolButton>
#include <QVBoxLayout>

#include "TourApi.h"
#include "core/VideoPlayer.h"

namespace rustcam {

namespace {
constexpr int kWatchdogIntervalMs = 500;
constexpr int kStalledTicks = 6;  ///< 连续 3 秒没有新帧即判定 stalled
} // namespace

TourCell::TourCell(const QString& layoutItemId, QWidget* parent)
    : QWidget(parent)
    , m_layoutItemId(layoutItemId)
{
    setObjectName(QStringLiteral("TourCell"));

    QVBoxLayout* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_player = new VideoPlayer(this);
    layout->addWidget(m_player, 1);

    QWidget* bar = new QWidget(this);
    bar->setObjectName(QStringLiteral("TourCellBar"));
    QHBoxLayout* barLayout = new QHBoxLayout(bar);
    barLayout->setContentsMargins(4, 2, 4, 2);
    barLayout->setSpacing(4);

    m_titleLabel = new QLabel(bar);
    m_titleLabel->setObjectName(QStringLiteral("TourCellTitle"));
    barLayout->addWidget(m_titleLabel, 1);

    m_statusLabel = new QLabel(bar);
    m_statusLabel->setObjectName(QStringLiteral("TourCellStatus"));
    barLayout->addWidget(m_statusLabel, 0);

    m_fullscreenButton = new QToolButton(bar);
    m_fullscreenButton->setText(QStringLiteral("⛶"));
    m_fullscreenButton->setToolTip(QStringLiteral("全屏"));
    m_fullscreenButton->setAutoRaise(true);
    barLayout->addWidget(m_fullscreenButton, 0);

    m_audioButton = new QToolButton(bar);
    m_audioButton->setText(QStringLiteral("🔇"));
    m_audioButton->setToolTip(QStringLiteral("音频"));
    m_audioButton->setCheckable(true);
    m_audioButton->setAutoRaise(true);
    barLayout->addWidget(m_audioButton, 0);

    m_retryButton = new QToolButton(bar);
    m_retryButton->setText(QStringLiteral("⟳"));
    m_retryButton->setToolTip(QStringLiteral("重试"));
    m_retryButton->setAutoRaise(true);
    barLayout->addWidget(m_retryButton, 0);

    layout->addWidget(bar, 0);

    connect(m_fullscreenButton, &QToolButton::clicked, this, [this]() {
        emit fullscreenRequested(m_layoutItemId);
    });
    connect(m_audioButton, &QToolButton::toggled, this, [this](bool checked) {
        emit audioToggled(m_layoutItemId, checked);
    });
    connect(m_retryButton, &QToolButton::clicked, this, [this]() {
        emit retryRequested(m_layoutItemId);
    });
}

QString TourCell::title() const
{
    return m_titleLabel ? m_titleLabel->text() : QString();
}

QString TourCell::statusText() const
{
    return m_statusLabel ? m_statusLabel->text() : QString();
}

void TourCell::setChannelTitle(const QString& text)
{
    m_titleLabel->setText(text);
    if (m_player) {
        m_player->setDeviceName(text);
    }
}

void TourCell::setStatusText(const QString& text)
{
    m_statusLabel->setText(text);
}

void TourCell::setAudioEnabled(bool enabled)
{
    if (m_audioButton->isChecked() != enabled) {
        m_audioButton->setChecked(enabled);
    }
    if (m_player) {
        m_player->setAudioEnabled(enabled);
    }
}

TourPlayerManager::TourPlayerManager(TourApi* api, QWidget* gridHost, QObject* parent)
    : QObject(parent)
    , m_api(api)
    , m_host(gridHost)
{
    m_watchdog = new QTimer(this);
    m_watchdog->setInterval(kWatchdogIntervalMs);
    connect(m_watchdog, &QTimer::timeout, this, &TourPlayerManager::onWatchdog);
    m_watchdog->start();
}

TourPlayerManager::~TourPlayerManager()
{
    releaseAll();
}

void TourPlayerManager::setLayout(const PlayerLayout& layout)
{
    releaseAll();

    if (!m_host) {
        return;
    }

    if (!m_grid) {
        m_grid = new QGridLayout(m_host);
        m_grid->setContentsMargins(0, 0, 0, 0);
        m_grid->setSpacing(2);
        m_host->setLayout(m_grid);
    } else {
        while (m_grid->count() > 0) {
            QLayoutItem* item = m_grid->takeAt(0);
            if (!item) {
                break;
            }
            if (item->widget()) {
                item->widget()->deleteLater();
            }
            delete item;
        }
    }
    m_cells.clear();
    m_states.clear();

    for (const LayoutItem& item : layout.layoutJson) {
        TourCell* cell = new TourCell(item.id, m_host);
        m_grid->addWidget(cell, item.row, item.col, item.rowSpan, item.colSpan);
        m_cells.insert(item.id, cell);
        m_states.insert(item.id, SlotState());

        connect(cell, &TourCell::retryRequested, this, &TourPlayerManager::retryRequested);
        connect(cell, &TourCell::fullscreenRequested, this, [this](const QString& layoutItemId) {
            toggleFullscreen(layoutItemId);
        });
        connect(cell, &TourCell::audioToggled, this, [this](const QString& layoutItemId, bool enabled) {
            setAudio(layoutItemId, enabled);
        });
        connect(cell->player(), &VideoPlayer::started, this, [this, item]() {
            onCellStarted(item.id);
        });
        connect(cell->player(), &VideoPlayer::error, this, [this, item](const QString& reason) {
            onCellError(item.id, reason);
        });
        connect(cell->player(), &VideoPlayer::disconnected, this, [this, item]() {
            onCellDisconnected(item.id);
        });
        connect(cell->player(), &VideoPlayer::fullscreenToggled, this,
                [this, item](bool fullscreen) { onCellFullscreen(item.id, fullscreen); });
    }
}

void TourPlayerManager::play(const QString& layoutItemId, const TourChannelRef& channel, quint64 generation)
{
    TourCell* cell = m_cells.value(layoutItemId, nullptr);
    if (!cell || !m_api) {
        return;
    }

    SlotState& state = m_states[layoutItemId];
    state.generation = generation;
    state.playing = false;
    state.stalledTicks = 0;
    state.lastFrames = -1;

    cell->setChannelTitle(channel.deviceTag + QLatin1Char('/') + channel.channelTag);
    cell->setStatusText(QStringLiteral("请求中"));

    const quint64 requestGeneration = generation;
    m_api->fetchPlayUrl(channel, [this, layoutItemId, requestGeneration](bool success, const QString& url,
                                                                         const QString& error) {
        onPlayUrlReady(layoutItemId, requestGeneration, success, url, error);
    });
}

void TourPlayerManager::onPlayUrlReady(const QString& layoutItemId, quint64 generation, bool success,
                                       const QString& url, const QString& error)
{
    TourCell* cell = m_cells.value(layoutItemId, nullptr);
    if (!cell) {
        return;
    }
    if (m_states.value(layoutItemId).generation != generation) {
        // 过期请求：丢弃。
        return;
    }
    if (!success) {
        cell->setStatusText(QStringLiteral("失败"));
        emit playFailed(layoutItemId, generation, error);
        return;
    }
    cell->setStatusText(QStringLiteral("首帧加载"));
    cell->player()->play(url);
}

void TourPlayerManager::releaseSlot(const QString& layoutItemId)
{
    TourCell* cell = m_cells.value(layoutItemId, nullptr);
    if (!cell) {
        return;
    }
    cell->player()->stop();
    cell->setStatusText(QString());
    SlotState& state = m_states[layoutItemId];
    state.playing = false;
    state.stalledTicks = 0;
    state.lastFrames = -1;
}

void TourPlayerManager::releaseAll()
{
    for (auto it = m_cells.constBegin(); it != m_cells.constEnd(); ++it) {
        if (it.value()->player()) {
            it.value()->player()->stop();
        }
    }
    for (auto it = m_states.begin(); it != m_states.end(); ++it) {
        it->playing = false;
        it->stalledTicks = 0;
        it->lastFrames = -1;
    }
}

void TourPlayerManager::setAudio(const QString& layoutItemId, bool enabled)
{
    for (auto it = m_cells.constBegin(); it != m_cells.constEnd(); ++it) {
        const bool isTarget = it.key() == layoutItemId;
        it.value()->setAudioEnabled(isTarget && enabled);
    }
}

void TourPlayerManager::toggleFullscreen(const QString& layoutItemId)
{
    TourCell* cell = m_cells.value(layoutItemId, nullptr);
    if (cell) {
        cell->player()->toggleFullscreen();
    }
}

VideoPlayer* TourPlayerManager::playerFor(const QString& layoutItemId) const
{
    TourCell* cell = m_cells.value(layoutItemId, nullptr);
    return cell ? cell->player() : nullptr;
}

QString TourPlayerManager::channelTitle(const QString& layoutItemId) const
{
    TourCell* cell = m_cells.value(layoutItemId, nullptr);
    return cell ? cell->title() : QString();
}

void TourPlayerManager::onCellStarted(const QString& layoutItemId)
{
    TourCell* cell = m_cells.value(layoutItemId, nullptr);
    if (!cell) {
        return;
    }
    SlotState& state = m_states[layoutItemId];
    state.playing = true;
    state.stalledTicks = 0;
    state.lastFrames = -1;
    cell->setStatusText(QStringLiteral("播放中"));
    emit firstFrame(layoutItemId, state.generation);
}

void TourPlayerManager::onCellError(const QString& layoutItemId, const QString& reason)
{
    TourCell* cell = m_cells.value(layoutItemId, nullptr);
    if (cell) {
        cell->setStatusText(QStringLiteral("失败"));
    }
    const quint64 generation = m_states.value(layoutItemId).generation;
    emit playFailed(layoutItemId, generation, reason);
}

void TourPlayerManager::onCellDisconnected(const QString& layoutItemId)
{
    const quint64 generation = m_states.value(layoutItemId).generation;
    emit disconnected(layoutItemId, generation);
}

void TourPlayerManager::onCellFullscreen(const QString& layoutItemId, bool fullscreen)
{
    emit fullscreenChanged(layoutItemId, fullscreen);
}

void TourPlayerManager::onWatchdog()
{
    for (auto it = m_cells.constBegin(); it != m_cells.constEnd(); ++it) {
        VideoPlayer* player = it.value()->player();
        if (!player || !player->player()) {
            continue;
        }
        SlotState& state = m_states[it.key()];
        if (!state.playing || player->state() != VideoPlayer::State::Playing) {
            state.stalledTicks = 0;
            state.lastFrames = -1;
            continue;
        }
        const qint64 frames = static_cast<qint64>(
            player->player()->get_stats().frames_rendererd.load());
        if (state.lastFrames >= 0 && frames == state.lastFrames) {
            ++state.stalledTicks;
            if (state.stalledTicks >= kStalledTicks) {
                state.stalledTicks = 0;
                emit stalled(it.key(), state.generation);
            }
        } else {
            state.stalledTicks = 0;
        }
        state.lastFrames = frames;
    }
}

} // namespace rustcam
