#pragma once

#include "TourTypes.h"

#include <QString>
#include <QWidget>

class QLabel;
class QToolButton;

namespace rustcam {

class TourApi;
class VideoPlayer;

/// 轮询窗口：布局里的一个窗口位置。
///
/// 与实时播放器的差异全部封装在这里，外部只按"轮询窗口"语义使用：
///  - 不显示实时播放器的悬浮工具条（其中"停止"会让调度器与画面脱节）；
///  - 不接受设备拖放、不允许拖动换位（通道分配由调度器独占）；
///  - 角标显示轮询状态（待轮询/请求中/首帧加载/播放中/失败/已跳过/已锁定/补位中）；
///  - 播放回调带 generation，过期回调直接丢弃；
///  - 画面停滞由看门狗判定，交给调度器补位。
class TourPlayer : public QWidget {
    Q_OBJECT
public:
    explicit TourPlayer(TourApi* api, const QString& layoutItemId, QWidget* parent = nullptr);
    ~TourPlayer() override;

    QString layoutItemId() const { return m_layoutItemId; }
    /// 当前生效的 generation；0 表示常驻锁定窗口（不受调度器管理）。
    quint64 generation() const { return m_generation; }

    /// 播放一路通道；generation 用于丢弃过期回调。
    void play(const TourChannelRef& channel, quint64 generation);
    /// 释放当前通道，保留窗口本身。
    void release();
    /// 设置窗口角标状态与说明。
    void setSlotState(TourChannelState state, const QString& reason = QString());

    VideoPlayer* video() const { return m_video; }
    QString title() const;
    /// 音频开关：同一时刻只允许一个轮询窗口出声。
    void setAudioEnabled(bool enabled);
    /// 看门狗：判定画面是否停滞（帧数不再增长）。返回 true 表示本次判定为停滞。
    bool detectStall();

signals:
    void firstFrame(quint64 generation);
    void failed(quint64 generation, const QString& reason);
    void disconnected(quint64 generation);
    /// 用户点击窗口上的重试按钮。
    void retryRequested();
    void fullscreenToggled(bool fullscreen);
    void audioToggled(bool enabled);

private slots:
    void onRetryClicked();
    void onFullscreenClicked();
    void onAudioToggled(bool checked);

private:
    void onPlayUrlReady(quint64 generation, quint64 requestSeq, bool success,
                        const QString& url, const QString& error);
    void updateStatusText();
    void updateAudioIcon();

    TourApi* m_api = nullptr;
    QString m_layoutItemId;
    quint64 m_generation = 0;
    quint64 m_requestSeq = 0;
    bool m_playing = false;
    qint64 m_lastFrames = -1;
    int m_stalledTicks = 0;

    TourChannelState m_state = TourChannelState::Pending;
    QString m_reason;

    VideoPlayer* m_video = nullptr;
    QLabel* m_titleLabel = nullptr;
    QLabel* m_statusLabel = nullptr;
    QToolButton* m_fullscreenButton = nullptr;
    QToolButton* m_audioButton = nullptr;
    QToolButton* m_retryButton = nullptr;
};

} // namespace rustcam
