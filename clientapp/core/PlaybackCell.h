#pragma once

#include <QWidget>
#include <QString>
#include <QResizeEvent>
#include "CoreExport.h"

class QDragEnterEvent;
class QDragMoveEvent;
class QDropEvent;

namespace rustcam {

class PlaybackPlayerWidget;

/// 播放格子：布局里的一个位置，里面放一个可移动的播放器
class RUSTCAM_CORE_API PlaybackCell : public QWidget {
    Q_OBJECT
public:
    explicit PlaybackCell(QWidget* parent = nullptr);
    ~PlaybackCell() override;

    /// 布局项 id（格子在布局中的位置标识）
    QString layoutItemId() const { return m_layoutItemId; }
    void setLayoutItemId(const QString& id) { m_layoutItemId = id; }

    /// 格子里放的播放器
    PlaybackPlayerWidget* playerWidget() const { return m_playerWidget; }

    void setSelected(bool selected);

signals:
    /// 格子被点击（带布局项 id 与当前通道标识）
    void clicked(const QString& layoutItemId, const QString& deviceId);
    /// 播放进度（绝对时间，毫秒），由格子里的播放器转发
    void playbackProgressChanged(const QString& layoutItemId, int64_t absoluteTimeMs);
    void closeRequested(const QString& layoutItemId);
    /// 有播放器被拖到了本格子上（sourceUid 为被拖动播放器的 uid）
    void playerDropped(const QString& layoutItemId, const QString& sourceUid);
    /// 录像文件被拖到了本格子上
    void recordingDropped(const QString& layoutItemId, const QString& channelKey, int64_t startTimeMs);
    /// 设备树通道被拖到了本格子上
    void channelDropped(const QString& layoutItemId, const QString& deviceTag,
                        const QString& channelTag, const QString& displayName);

protected:
    void dragEnterEvent(QDragEnterEvent* event) override;
    void dragMoveEvent(QDragMoveEvent* event) override;
    void dropEvent(QDropEvent* event) override;
    void resizeEvent(QResizeEvent* event) override;
    void paintEvent(QPaintEvent* event) override;

private:
    void onPlayerClicked(const QString& deviceId);
    void onPlayerProgress(int64_t absoluteTimeMs);

private:
    QString m_layoutItemId;
    PlaybackPlayerWidget* m_playerWidget = nullptr;
    bool m_selected = false;
};

}
