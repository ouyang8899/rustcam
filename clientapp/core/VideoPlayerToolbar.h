#pragma once

#include <QWidget>
#include <QToolButton>
#include <QMenu>
#include <QAction>
#include <QLabel>
#include <QDoubleSpinBox>
#include <QCheckBox>
#include <QHBoxLayout>
#include <QVBoxLayout>
#include <QMouseEvent>
#include <QWheelEvent>
#include <QEvent>
#include <QTimer>
#include "CoreExport.h"

namespace rustcam {
class VideoPlayer;
}

namespace rustcam {

class RUSTCAM_CORE_API VideoPlayerToolbar : public QWidget {
    Q_OBJECT
public:
    explicit VideoPlayerToolbar(QWidget* parent = nullptr);
    ~VideoPlayerToolbar();

    void setVideoPlayer(VideoPlayer* player);
    VideoPlayer* videoPlayer() const { return m_videoPlayer; }

    void play(const QString& url);
    void stop();
    bool isPlaying() const;

    void setSelected(bool selected);
    void setWatermarkAccount(const QString& account);
    void setDeviceName(const QString& deviceName);
    void toggleFullscreen();
    void toggleAudio();

signals:
    void clicked();

protected:
    void mousePressEvent(QMouseEvent* event) override;
    void mouseMoveEvent(QMouseEvent* event) override;
    void wheelEvent(QWheelEvent* event) override;
    void resizeEvent(QResizeEvent* event) override;
    bool eventFilter(QObject* obj, QEvent* event) override;

private slots:
    void onStopClicked();
    void onRotateAction(QAction* action);
    void onMirrorAction(QAction* action);
    void onScreenshotClicked();
    void onZoomToggled(bool checked);
    void onZoomValueChanged(double value);
    void onFullscreenClicked();
    void onAudioClicked();
    void onMediaRateClicked();
    void onThemeChanged();

private:
    void setupUi();
    void createRotationMenu();
    void createMirrorMenu();
    void createZoomMenu();
    void createCollapseMenu();
    void updateZoomLabel();
    void updateAudioIcon();
    void updateCollapseState();
    void refreshIcons();
    QPoint mapToVideoPlayer(const QPoint& pos);
    int calculateButtonsWidth() const;

    VideoPlayer* m_videoPlayer = nullptr;
    QWidget* m_toolbarWidget = nullptr;
    QHBoxLayout* m_toolbarLayout = nullptr;

    QToolButton* m_stopBtn = nullptr;
    QToolButton* m_rotateBtn = nullptr;
    QMenu* m_rotateMenu = nullptr;
    QToolButton* m_mirrorBtn = nullptr;
    QMenu* m_mirrorMenu = nullptr;
    QToolButton* m_screenshotBtn = nullptr;
    QToolButton* m_zoomBtn = nullptr;
    QMenu* m_zoomMenu = nullptr;
    QToolButton* m_fullscreenBtn = nullptr;
    QToolButton* m_audioBtn = nullptr;
    QToolButton* m_mediaRateBtn = nullptr;
    QToolButton* m_moreBtn = nullptr;
    QMenu* m_collapseMenu = nullptr;

    QList<QWidget*> m_primaryButtons;
    QList<QWidget*> m_secondaryButtons;

    QCheckBox* m_zoomCheckBox = nullptr;
    QDoubleSpinBox* m_zoomSpinBox = nullptr;
    QLabel* m_zoomLabel = nullptr;

    bool m_zoomEnabled = false;
    int m_rotationAngle = 0;
    bool m_mirrorH = false;
    bool m_mirrorV = false;
    bool m_audioEnabled = true;
    bool m_mediaRateEnabled = true;
    QColor m_iconColor = Qt::white;

    QPoint m_dragPosition;
    bool m_isDragging = false;
};

}
