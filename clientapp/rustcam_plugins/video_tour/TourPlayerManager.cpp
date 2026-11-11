#include "TourPlayerManager.h"

#include <QEvent>
#include <QTimer>

#include "TourApi.h"
#include "TourPlayer.h"
#include "core/VideoPlayer.h"
namespace rustcam {

namespace {
constexpr int kWatchdogIntervalMs = 500;

/// 格子之间的间隔与画布留白。
constexpr int kGridMargin = 2;
constexpr int kGridSpacing = 2;
} // namespace

TourPlayerManager::TourPlayerManager(TourApi* api, QWidget* gridHost, QObject* parent)
    : QObject(parent)
    , m_api(api)
    , m_host(gridHost)
{
    if (m_host) {
        // 宿主尺寸变化时按新尺寸重排格子。
        m_host->installEventFilter(this);
    }
    m_watchdog = new QTimer(this);
    m_watchdog->setInterval(kWatchdogIntervalMs);
    connect(m_watchdog, &QTimer::timeout, this, &TourPlayerManager::onWatchdog);
    m_watchdog->start();
}

TourPlayerManager::~TourPlayerManager()
{
    releaseAll();
}

bool TourPlayerManager::eventFilter(QObject* watched, QEvent* event)
{
    if (watched == m_host) {
        // 宿主尺寸变化时按新尺寸重排格子；首次显示时同样需要重排，
        // 否则在还没拿到真实尺寸前创建的格子会停在 0×0。
        if (event->type() == QEvent::Resize || event->type() == QEvent::Show) {
            applyLayout();
        }
    }
    return QObject::eventFilter(watched, event);
}

void TourPlayerManager::setLayout(const PlayerLayout& layout)
{
    releaseAll();

    for (auto it = m_cells.constBegin(); it != m_cells.constEnd(); ++it) {
        delete it.value();
    }
    m_cells.clear();
    m_layout = layout;

    if (!m_host) {
        return;
    }

    for (const LayoutItem& item : layout.layoutJson) {
        TourPlayer* cell = new TourPlayer(m_api, item.id, m_host);
        m_cells.insert(item.id, cell);

        connect(cell, &TourPlayer::retryRequested, this, [this, item]() {
            emit retryRequested(item.id);
        });
        connect(cell, &TourPlayer::fullscreenToggled, this, [this, item](bool fullscreen) {
            emit fullscreenChanged(item.id, fullscreen);
        });
        connect(cell, &TourPlayer::audioToggled, this, [this, item](bool enabled) {
            setAudio(item.id, enabled);
        });
        connect(cell, &TourPlayer::firstFrame, this, [this, item](quint64 generation) {
            emit firstFrame(item.id, generation);
        });
        connect(cell, &TourPlayer::failed, this, [this, item](quint64 generation, const QString& reason) {
            emit playFailed(item.id, generation, reason);
        });
        connect(cell, &TourPlayer::disconnected, this, [this, item](quint64 generation) {
            emit disconnected(item.id, generation);
        });
    }

    applyLayout();
}

void TourPlayerManager::applyLayout()
{
    if (!m_host || m_cells.isEmpty()) {
        return;
    }

    const QRect area = m_host->contentsRect();
    if (area.width() <= 0 || area.height() <= 0) {
        return;
    }

    const int rows = qMax(1, m_layout.rows);
    const int cols = qMax(1, m_layout.cols);
    const int cellW = (area.width() - kGridMargin * 2 - kGridSpacing * (cols - 1)) / cols;
    const int cellH = (area.height() - kGridMargin * 2 - kGridSpacing * (rows - 1)) / rows;
    if (cellW <= 0 || cellH <= 0) {
        return;
    }

    for (const LayoutItem& item : m_layout.layoutJson) {
        TourPlayer* cell = m_cells.value(item.id, nullptr);
        if (!cell) {
            continue;
        }
        const int x = kGridMargin + item.col * (cellW + kGridSpacing);
        const int y = kGridMargin + item.row * (cellH + kGridSpacing);
        const int w = item.colSpan * cellW + (item.colSpan - 1) * kGridSpacing;
        const int h = item.rowSpan * cellH + (item.rowSpan - 1) * kGridSpacing;
        cell->setGeometry(x, y, w, h);
        cell->show();
    }
}

void TourPlayerManager::play(const QString& layoutItemId, const TourChannelRef& channel, quint64 generation)
{
    TourPlayer* cell = m_cells.value(layoutItemId, nullptr);
    if (!cell) {
        return;
    }
    cell->play(channel, generation);
}

void TourPlayerManager::releaseSlot(const QString& layoutItemId)
{
    TourPlayer* cell = m_cells.value(layoutItemId, nullptr);
    if (!cell) {
        return;
    }
    cell->release();
}

void TourPlayerManager::setSlotState(const QString& layoutItemId, TourChannelState state, const QString& reason)
{
    TourPlayer* cell = m_cells.value(layoutItemId, nullptr);
    if (cell) {
        cell->setSlotState(state, reason);
    }
}

void TourPlayerManager::releaseAll()
{
    for (auto it = m_cells.constBegin(); it != m_cells.constEnd(); ++it) {
        it.value()->release();
    }
}

void TourPlayerManager::setAudio(const QString& layoutItemId, bool enabled)
{
    for (auto it = m_cells.constBegin(); it != m_cells.constEnd(); ++it) {
        it.value()->setAudioEnabled(it.key() == layoutItemId && enabled);
    }
}

void TourPlayerManager::toggleFullscreen(const QString& layoutItemId)
{
    TourPlayer* cell = m_cells.value(layoutItemId, nullptr);
    if (cell) {
        cell->video()->toggleFullscreen();
    }
}

TourPlayer* TourPlayerManager::playerFor(const QString& layoutItemId) const
{
    return m_cells.value(layoutItemId, nullptr);
}

QString TourPlayerManager::channelTitle(const QString& layoutItemId) const
{
    TourPlayer* cell = m_cells.value(layoutItemId, nullptr);
    return cell ? cell->title() : QString();
}

void TourPlayerManager::onWatchdog()
{
    for (auto it = m_cells.constBegin(); it != m_cells.constEnd(); ++it) {
        if (it.value()->detectStall()) {
            emit stalled(it.key(), it.value()->generation());
        }
    }
}

} // namespace rustcam
