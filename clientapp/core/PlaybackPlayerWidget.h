#pragma once

#include <QWidget>
#include <QLabel>
#include <QString>
#include <QList>
#include <QPoint>
#include "CoreExport.h"
#include "StreamSource.h"

class QEvent;
class QMouseEvent;

namespace rustcam {

class ReplayPlayer;
class PlaybackController;
class PlaybackGroup;

/// 一个可移动的播放器：画面(ReplayPlayer) + 片源(StreamSource) + 通道绑定
/// 可以在格子(PlaybackCell)之间切换，通道绑定跟着它走
class RUSTCAM_CORE_API PlaybackPlayerWidget : public QWidget {
    Q_OBJECT
public:
    explicit PlaybackPlayerWidget(QWidget* parent = nullptr);
    ~PlaybackPlayerWidget() override;

    /// 播放器的唯一标识（换格子也不变，时间轴播放头用它做键）
    QString uid() const { return m_uid; }

    /// 当前播的通道（"deviceTag/channelTag"）
    QString channelKey() const { return m_channelKey; }
    void setChannelKey(const QString& key) { m_channelKey = key; }

    const QString& deviceId() const { return m_deviceId; }

    void bindDevice(const QString& deviceId);
    void loadClips(const QList<Clip>& clips);
    void setController(PlaybackController* controller);
    void setGroup(PlaybackGroup* group);
    void setSelected(bool selected);

    PlaybackController* controller() const { return m_controller; }
    PlaybackGroup* group() const { return m_group; }
    StreamSource* source() const { return m_source; }
    ReplayPlayer* player() const { return m_player; }

    void showNoRecording(int64_t timeMs);

signals:
    /// 画面被点击（带当前绑定的通道标识）
    void clicked(const QString& deviceId);
    void playbackProgressChanged(int64_t absoluteTimeMs);
    void closeRequested();

protected:
    bool eventFilter(QObject* watched, QEvent* event) override;
    void mousePressEvent(QMouseEvent* event) override;
    void mouseMoveEvent(QMouseEvent* event) override;
    void resizeEvent(QResizeEvent* event) override;

private:
    void startDrag();
    void onPlayerClicked();
    void onSourceProgress(int64_t absoluteTimeMs);

private:
    QString m_uid;
    QString m_channelKey;
    QString m_deviceId;
    ReplayPlayer* m_player;
    StreamSource* m_source;
    PlaybackController* m_controller;
    PlaybackGroup* m_group;
    QLabel* m_noRecordingLabel;
    bool m_selected = false;
    /// 按下位置（用于判断是否变成拖动）
    QPoint m_pressPos;
    bool m_pressed = false;
};

}
