#pragma once

#include "TourTypes.h"

#include <QMap>
#include <QObject>
#include <QString>
#include <QWidget>

#include "PlayerLayout.h"

class QGridLayout;
class QLabel;
class QTimer;
class QToolButton;

namespace rustcam {

class VideoPlayer;
class TourApi;

/// 网格里的一个格子：画面 + 通道名 + 状态角标 + 全屏/音频/重试按钮。
class TourCell : public QWidget {
    Q_OBJECT
public:
    explicit TourCell(const QString& layoutItemId, QWidget* parent = nullptr);

    QString layoutItemId() const { return m_layoutItemId; }
    VideoPlayer* player() const { return m_player; }
    QString title() const;
    QString statusText() const;

    void setChannelTitle(const QString& text);
    void setStatusText(const QString& text);
    void setAudioEnabled(bool enabled);

signals:
    void retryRequested(const QString& layoutItemId);
    void fullscreenRequested(const QString& layoutItemId);
    void audioToggled(const QString& layoutItemId, bool enabled);

private:
    QString m_layoutItemId;
    VideoPlayer* m_player = nullptr;
    QLabel* m_titleLabel = nullptr;
    QLabel* m_statusLabel = nullptr;
    QToolButton* m_fullscreenButton = nullptr;
    QToolButton* m_audioButton = nullptr;
    QToolButton* m_retryButton = nullptr;
};

/// 播放器池：按布局创建 VideoPlayer，负责取流地址、看门狗与音频/全屏控制。
class TourPlayerManager : public QObject {
    Q_OBJECT
public:
    explicit TourPlayerManager(TourApi* api, QWidget* gridHost, QObject* parent = nullptr);
    ~TourPlayerManager() override;

    /// 按布局重建网格。
    void setLayout(const PlayerLayout& layout);

    /// 在指定格子播放某路通道；generation 用于丢弃过期回调。
    void play(const QString& layoutItemId, const TourChannelRef& channel, quint64 generation);
    void releaseSlot(const QString& layoutItemId);
    void releaseAll();

    void setAudio(const QString& layoutItemId, bool enabled);
    void toggleFullscreen(const QString& layoutItemId);

    VideoPlayer* playerFor(const QString& layoutItemId) const;
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

private:
    struct SlotState {
        quint64 generation = 0;
        qint64 lastFrames = -1;
        int stalledTicks = 0;
        bool playing = false;
    };

    void onPlayUrlReady(const QString& layoutItemId, quint64 generation, bool success,
                        const QString& url, const QString& error);
    void onCellStarted(const QString& layoutItemId);
    void onCellError(const QString& layoutItemId, const QString& reason);
    void onCellDisconnected(const QString& layoutItemId);
    void onCellFullscreen(const QString& layoutItemId, bool fullscreen);

    TourApi* m_api = nullptr;
    QWidget* m_host = nullptr;
    QGridLayout* m_grid = nullptr;
    QMap<QString, TourCell*> m_cells;
    QMap<QString, SlotState> m_states;
    QTimer* m_watchdog = nullptr;
};

} // namespace rustcam
