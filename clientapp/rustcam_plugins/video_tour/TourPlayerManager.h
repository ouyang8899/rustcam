#pragma once

#include "TourTypes.h"

#include <QMap>
#include <QObject>
#include <QString>
#include <QWidget>

#include "PlayerLayout.h"

class QTimer;

namespace rustcam {

class TourApi;
class TourPlayer;

/// 播放器池：按布局创建轮询窗口，负责取流地址、看门狗与音频/全屏控制。
///
/// 格子按布局绝对定位：宿主尺寸变化时重新计算每个格子的矩形，
/// 不使用 QGridLayout，避免网格与画布两套坐标系。
class TourPlayerManager : public QObject {
    Q_OBJECT
public:
    explicit TourPlayerManager(TourApi* api, QWidget* gridHost, QObject* parent = nullptr);
    ~TourPlayerManager() override;

    /// 按布局重建格子。
    void setLayout(const PlayerLayout& layout);

    /// 在指定格子播放某路通道；generation 用于丢弃过期回调。
    void play(const QString& layoutItemId, const TourChannelRef& channel, quint64 generation);
    void releaseSlot(const QString& layoutItemId);
    void releaseAll();

    /// 设置窗口角标状态（由调度器状态驱动）。
    void setSlotState(const QString& layoutItemId, TourChannelState state, const QString& reason = QString());

    /// 同一时刻只让一个窗口出声。
    void setAudio(const QString& layoutItemId, bool enabled);
    void toggleFullscreen(const QString& layoutItemId);

    TourPlayer* playerFor(const QString& layoutItemId) const;
    QString channelTitle(const QString& layoutItemId) const;

signals:
    void firstFrame(const QString& layoutItemId, quint64 generation);
    void playFailed(const QString& layoutItemId, quint64 generation, const QString& reason);
    void disconnected(const QString& layoutItemId, quint64 generation);
    void stalled(const QString& layoutItemId, quint64 generation);
    void fullscreenChanged(const QString& layoutItemId, bool fullscreen);
    void retryRequested(const QString& layoutItemId);

private slots:
    void onWatchdog();

protected:
    bool eventFilter(QObject* watched, QEvent* event) override;

private:
    /// 按当前宿主尺寸重算所有格子的位置与大小。
    void applyLayout();

    TourApi* m_api = nullptr;
    QWidget* m_host = nullptr;
    PlayerLayout m_layout;
    QMap<QString, TourPlayer*> m_cells;
    QTimer* m_watchdog = nullptr;
};

} // namespace rustcam
