#include "TourPlayer.h"

#include <QEvent>
#include <QHBoxLayout>
#include <QLabel>
#include <QMouseEvent>
#include <QToolButton>
#include <QVBoxLayout>

#include "TourApi.h"
#include "core/VideoPlayer.h"

namespace rustcam {

namespace {

/// 连续多少个看门狗周期帧数不增长即判定停滞（约 3 秒）。
constexpr int kStalledTicks = 6;

/// 轮询窗口用的画面控件：屏蔽实时播放器专有的交互。
///
/// 实时播放器支持悬浮工具条、拖放换台、拖动换位，这些在轮询里都由调度器独占，
/// 放开会让画面与调度状态脱节。这里不改 core/VideoPlayer，只在事件层拦截。
class TourVideoSurface : public VideoPlayer {
public:
    explicit TourVideoSurface(QWidget* parent)
        : VideoPlayer(parent)
    {
        // 轮询窗口不接受设备拖放：通道分配由调度器决定。
        setAcceptDrops(false);
    }

protected:
    bool eventFilter(QObject* watched, QEvent* event) override
    {
        if (watched == this) {
            // 屏蔽鼠标进入：实时播放器的悬浮工具条不会弹出，
            // 轮询窗口用的是自己的状态条。
            if (event->type() == QEvent::Enter) {
                return true;
            }
            // 屏蔽按下：避免把播放器拖到别的格子里。
            if (event->type() == QEvent::MouseButtonPress) {
                return true;
            }
        }
        return VideoPlayer::eventFilter(watched, event);
    }
};

} // namespace

TourPlayer::TourPlayer(TourApi* api, const QString& layoutItemId, QWidget* parent)
    : QWidget(parent)
    , m_api(api)
    , m_layoutItemId(layoutItemId)
{
    setObjectName(QStringLiteral("TourPlayer"));

    QVBoxLayout* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_video = new TourVideoSurface(this);
    m_video->installEventFilter(m_video);
    layout->addWidget(m_video, 1);

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

    // 轮询默认静音：同一时刻只允许一个窗口出声，由调度器侧切换。
    m_video->setAudioEnabled(false);
    updateAudioIcon();

    connect(m_fullscreenButton, &QToolButton::clicked, this, &TourPlayer::onFullscreenClicked);
    connect(m_audioButton, &QToolButton::toggled, this, &TourPlayer::onAudioToggled);
    connect(m_retryButton, &QToolButton::clicked, this, &TourPlayer::onRetryClicked);

    connect(m_video, &VideoPlayer::started, this, [this]() {
        m_playing = true;
        m_stalledTicks = 0;
        m_lastFrames = -1;
        updateStatusText();
        emit firstFrame(m_generation);
    });
    connect(m_video, &VideoPlayer::error, this, [this](const QString& reason) {
        emit failed(m_generation, reason);
    });
    connect(m_video, &VideoPlayer::disconnected, this, [this]() {
        emit disconnected(m_generation);
    });
    connect(m_video, &VideoPlayer::fullscreenToggled, this, &TourPlayer::fullscreenToggled);

    updateStatusText();
}

TourPlayer::~TourPlayer() = default;

QString TourPlayer::title() const
{
    return m_titleLabel ? m_titleLabel->text() : QString();
}

void TourPlayer::play(const TourChannelRef& channel, quint64 generation)
{
    m_generation = generation;
    m_playing = false;
    m_stalledTicks = 0;
    m_lastFrames = -1;

    const QString label = channel.deviceTag + QLatin1Char('/') + channel.channelTag;
    m_titleLabel->setText(label);
    m_video->setDeviceName(label);
    m_state = TourChannelState::Requesting;
    m_reason.clear();
    updateStatusText();

    if (!m_api) {
        return;
    }
    const quint64 requestGeneration = generation;
    m_api->fetchPlayUrl(channel, [this, requestGeneration](bool success, const QString& url,
                                                           const QString& error) {
        onPlayUrlReady(requestGeneration, success, url, error);
    });
}

void TourPlayer::onPlayUrlReady(quint64 generation, bool success, const QString& url, const QString& error)
{
    if (generation != m_generation) {
        // 过期请求：丢弃。
        return;
    }
    if (!success) {
        m_state = TourChannelState::Failed;
        m_reason = error;
        updateStatusText();
        emit failed(generation, error);
        return;
    }
    m_state = TourChannelState::FirstFrame;
    updateStatusText();
    m_video->play(url);
}

void TourPlayer::release()
{
    m_video->stop();
    m_playing = false;
    m_stalledTicks = 0;
    m_lastFrames = -1;
    m_titleLabel->clear();
    m_state = TourChannelState::Pending;
    m_reason.clear();
    updateStatusText();
}

void TourPlayer::setSlotState(TourChannelState state, const QString& reason)
{
    m_state = state;
    m_reason = reason;
    updateStatusText();
}

void TourPlayer::updateStatusText()
{
    QString text = tourChannelStateText(m_state);
    if (!m_reason.isEmpty()) {
        text += QStringLiteral("·") + m_reason;
    }
    m_statusLabel->setText(text);
}

bool TourPlayer::detectStall()
{
    if (!m_video || !m_video->player()) {
        return false;
    }
    if (!m_playing || m_video->state() != VideoPlayer::State::Playing) {
        m_stalledTicks = 0;
        m_lastFrames = -1;
        return false;
    }
    const qint64 frames = static_cast<qint64>(
        m_video->player()->get_stats().frames_rendererd.load());
    if (m_lastFrames >= 0 && frames == m_lastFrames) {
        ++m_stalledTicks;
        if (m_stalledTicks >= kStalledTicks) {
            m_stalledTicks = 0;
            return true;
        }
    } else {
        m_stalledTicks = 0;
    }
    m_lastFrames = frames;
    return false;
}

void TourPlayer::onRetryClicked()
{
    emit retryRequested();
}

void TourPlayer::onFullscreenClicked()
{
    // 全屏画面本身：与实时播放器一致，状态条留在原位。
    m_video->toggleFullscreen();
}

void TourPlayer::onAudioToggled(bool checked)
{
    m_video->setAudioEnabled(checked);
    updateAudioIcon();
    emit audioToggled(checked);
}

void TourPlayer::setAudioEnabled(bool enabled)
{
    if (m_audioButton->isChecked() != enabled) {
        // 程序化改状态时屏蔽信号，避免回到 onAudioToggled 形成回环。
        const bool blocked = m_audioButton->blockSignals(true);
        m_audioButton->setChecked(enabled);
        m_audioButton->blockSignals(blocked);
    }
    m_video->setAudioEnabled(enabled);
    updateAudioIcon();
}

void TourPlayer::updateAudioIcon()
{
    const bool on = m_audioButton->isChecked();
    m_audioButton->setText(on ? QStringLiteral("🔊") : QStringLiteral("🔇"));
}

} // namespace rustcam
