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

    int cellId() const { return m_cellId; }
    void setCellId(int id) { m_cellId = id; }

    /// 布局项 id（格子在布局中的位置标识）
    QString layoutItemId() const { return m_layoutItemId; }
    void setLayoutItemId(const QString& id) { m_layoutItemId = id; }

    /// 格子里放的播放器
    PlaybackPlayerWidget* playerWidget() const { return m_playerWidget; }
    /// 换播放器（把旧的那个摘下来返回）
    PlaybackPlayerWidget* takePlayerWidget();
    void setPlayerWidget(PlaybackPlayerWidget* widget);

    void setSelected(bool selected);

signals:
    void clicked();
    /// 播放进度（绝对时间，毫秒），由格子里的播放器转发
    void playbackProgressChanged(int64_t absoluteTimeMs);
    /// 有播放器被拖到了本格子上（sourceUid 为被拖动播放器的 uid）
    void playerDropped(const QString& sourceUid);
    /// 录像文件被拖到了本格子上
    void recordingDropped(const QString& channelKey, int64_t startTimeMs);

protected:
    void dragEnterEvent(QDragEnterEvent* event) override;
    void dragMoveEvent(QDragMoveEvent* event) override;
    void dropEvent(QDropEvent* event) override;
    void resizeEvent(QResizeEvent* event) override;
    void paintEvent(QPaintEvent* event) override;

private:
    int m_cellId = -1;
    QString m_layoutItemId;
    PlaybackPlayerWidget* m_playerWidget = nullptr;
    bool m_selected = false;
};

}
