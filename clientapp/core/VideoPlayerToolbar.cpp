#include "VideoPlayerToolbar.h"
#include "VideoPlayer.h"
#include "SystemConfig.h"
#include "ThemeManager.h"
#include "common/Logger.h"
#include <QFileDialog>
#include <QMessageBox>
#include <QApplication>
#include <QDateTime>
#include <QWidgetAction>
#include <QSvgRenderer>
#include <QPixmap>
#include <QPainter>
#include <QBitmap>

namespace rustcam {

static QIcon createColoredIcon(const QString& svgPath, const QColor& color) {
    QSvgRenderer renderer(svgPath);
    if (!renderer.isValid()) {
        return QIcon();
    }

    QImage image(24, 24, QImage::Format_ARGB32);
    image.fill(Qt::transparent);
    QPainter painter(&image);
    renderer.render(&painter);
    painter.end();

    for (int y = 0; y < image.height(); ++y) {
        for (int x = 0; x < image.width(); ++x) {
            QRgb pixel = image.pixel(x, y);
            if (qAlpha(pixel) > 0) {
                image.setPixel(x, y, color.rgba());
            }
        }
    }

    return QIcon(QPixmap::fromImage(image));
}

VideoPlayerToolbar::VideoPlayerToolbar(QWidget* parent)
    : QWidget(parent)
    , m_videoPlayer(nullptr)
    , m_toolbarWidget(nullptr)
    , m_stopBtn(nullptr)
    , m_rotateBtn(nullptr)
    , m_mirrorBtn(nullptr)
    , m_screenshotBtn(nullptr)
    , m_zoomBtn(nullptr)
    , m_zoomCheckBox(nullptr)
    , m_zoomSpinBox(nullptr)
    , m_zoomLabel(nullptr)
    , m_mediaRateBtn(nullptr)
    , m_moreBtn(nullptr)
    , m_collapseMenu(nullptr)
    , m_rotationAngle(0)
    , m_mirrorH(false)
    , m_mirrorV(false)
    , m_audioEnabled(true)
    , m_mediaRateEnabled(true)
{
    setObjectName("VideoPlayerToolbar");
    setAttribute(Qt::WA_StyledBackground);
    setAttribute(Qt::WA_NativeWindow);

    setStyleSheet(R"(
        #VideoPlayerToolbar {
            background-color: rgba(0, 0, 0, 180);
        }
        #ToolbarButton {
            background-color: transparent;
            border: none;
            padding: 4px;
            color: white;
        }
        #ToolbarButton:hover {
            background-color: rgba(255, 255, 255, 0.1);
            border-radius: 4px;
        }
        #ToolbarButton:pressed {
            background-color: rgba(255, 255, 255, 0.2);
            border-radius: 4px;
        }
        #VideoPlayerToolbarWidget {
            background-color: transparent;
        }
        QToolButton::menu-indicator {
            image: none;
        }
    )");

    QVBoxLayout* mainLayout = new QVBoxLayout(this);
    mainLayout->setContentsMargins(0, 0, 0, 0);
    mainLayout->setSpacing(0);

    m_toolbarWidget = new QWidget(this);
    m_toolbarWidget->setObjectName("VideoPlayerToolbarWidget");
    m_toolbarWidget->setFixedHeight(40);
    m_toolbarWidget->installEventFilter(this);

    m_toolbarLayout = new QHBoxLayout(m_toolbarWidget);
    m_toolbarLayout->setContentsMargins(4, 4, 4, 4);
    m_toolbarLayout->setSpacing(4);

    setupUi();

    mainLayout->addStretch();
    mainLayout->addWidget(m_toolbarWidget);

    connect(ThemeManager::instance(), &ThemeManager::themeChanged, this, &VideoPlayerToolbar::onThemeChanged);

    hide();
}

VideoPlayerToolbar::~VideoPlayerToolbar() {
    if (m_videoPlayer) {
        m_videoPlayer->removeEventFilter(this);
    }
}

void VideoPlayerToolbar::setupUi() {
    m_iconColor = ThemeManager::instance()->getColor("text_primary");
    if (!m_iconColor.isValid()) {
        m_iconColor = Qt::white;
    }

    m_stopBtn = new QToolButton(m_toolbarWidget);
    m_stopBtn->setObjectName("ToolbarButton");
    m_stopBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_stop.svg", m_iconColor));
    m_stopBtn->setToolTip(tr("Stop"));
    m_stopBtn->setFixedSize(32, 32);
    connect(m_stopBtn, &QToolButton::clicked, this, &VideoPlayerToolbar::onStopClicked);
    m_toolbarLayout->addWidget(m_stopBtn);

    m_rotateBtn = new QToolButton(m_toolbarWidget);
    m_rotateBtn->setObjectName("ToolbarButton");
    m_rotateBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_rotate.svg", m_iconColor));
    m_rotateBtn->setToolTip(tr("Rotate"));
    m_rotateBtn->setFixedSize(32, 32);
    createRotationMenu();
    m_rotateBtn->setMenu(m_rotateMenu);
    m_rotateBtn->setPopupMode(QToolButton::InstantPopup);
    m_toolbarLayout->addWidget(m_rotateBtn);

    m_mirrorBtn = new QToolButton(m_toolbarWidget);
    m_mirrorBtn->setObjectName("ToolbarButton");
    m_mirrorBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_mirror_h.svg", m_iconColor));
    m_mirrorBtn->setToolTip(tr("Mirror"));
    m_mirrorBtn->setFixedSize(32, 32);
    createMirrorMenu();
    m_mirrorBtn->setMenu(m_mirrorMenu);
    m_mirrorBtn->setPopupMode(QToolButton::InstantPopup);
    m_toolbarLayout->addWidget(m_mirrorBtn);

    m_screenshotBtn = new QToolButton(m_toolbarWidget);
    m_screenshotBtn->setObjectName("ToolbarButton");
    m_screenshotBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_screenshot.svg", m_iconColor));
    m_screenshotBtn->setToolTip(tr("Screenshot"));
    m_screenshotBtn->setFixedSize(32, 32);
    connect(m_screenshotBtn, &QToolButton::clicked, this, &VideoPlayerToolbar::onScreenshotClicked);
    m_toolbarLayout->addWidget(m_screenshotBtn);

    m_toolbarLayout->addSpacing(8);

    m_zoomBtn = new QToolButton(m_toolbarWidget);
    m_zoomBtn->setObjectName("ToolbarButton");
    m_zoomBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_zoom.svg", m_iconColor));
    m_zoomBtn->setToolTip(tr("Zoom (Mouse Wheel)"));
    m_zoomBtn->setFixedSize(32, 32);
    m_zoomBtn->setCheckable(true);
    m_zoomBtn->setChecked(false);
    connect(m_zoomBtn, &QToolButton::toggled, this, &VideoPlayerToolbar::onZoomToggled);
    m_toolbarLayout->addWidget(m_zoomBtn);

    m_toolbarLayout->addSpacing(8);

    m_audioBtn = new QToolButton(m_toolbarWidget);
    m_audioBtn->setObjectName("ToolbarButton");
    m_audioBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_audio_on.svg", m_iconColor));
    m_audioBtn->setToolTip(tr("Audio On/Off"));
    m_audioBtn->setFixedSize(32, 32);
    connect(m_audioBtn, &QToolButton::clicked, this, &VideoPlayerToolbar::onAudioClicked);
    m_toolbarLayout->addWidget(m_audioBtn);

    m_mediaRateBtn = new QToolButton(m_toolbarWidget);
    m_mediaRateBtn->setObjectName("ToolbarButton");
    m_mediaRateBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_mediainfo.svg", m_iconColor));
    m_mediaRateBtn->setToolTip(tr("Media Rate"));
    m_mediaRateBtn->setFixedSize(32, 32);
    m_mediaRateBtn->setCheckable(true);
    m_mediaRateBtn->setChecked(true);
    connect(m_mediaRateBtn, &QToolButton::clicked, this, &VideoPlayerToolbar::onMediaRateClicked);
    m_toolbarLayout->addWidget(m_mediaRateBtn);

    m_toolbarLayout->addSpacing(8);

    m_fullscreenBtn = new QToolButton(m_toolbarWidget);
    m_fullscreenBtn->setObjectName("ToolbarButton");
    m_fullscreenBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_fullscreen.svg", m_iconColor));
    m_fullscreenBtn->setToolTip(tr("Fullscreen"));
    m_fullscreenBtn->setFixedSize(32, 32);
    connect(m_fullscreenBtn, &QToolButton::clicked, this, &VideoPlayerToolbar::onFullscreenClicked);
    m_toolbarLayout->addWidget(m_fullscreenBtn);

    m_collapseMenu = new QMenu(m_toolbarWidget);
    m_moreBtn = new QToolButton(m_toolbarWidget);
    m_moreBtn->setObjectName("ToolbarButton");
    m_moreBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_more.svg", m_iconColor));
    m_moreBtn->setToolTip(tr("More"));
    m_moreBtn->setFixedSize(32, 32);
    m_moreBtn->setMenu(m_collapseMenu);
    m_moreBtn->setPopupMode(QToolButton::InstantPopup);
    m_moreBtn->hide();
    m_toolbarLayout->addWidget(m_moreBtn);

    m_toolbarLayout->addStretch();

    m_primaryButtons = {m_stopBtn, m_screenshotBtn, m_fullscreenBtn};
    m_secondaryButtons = {m_rotateBtn, m_mirrorBtn, m_zoomBtn, m_audioBtn, m_mediaRateBtn};

    refreshIcons();
}

void VideoPlayerToolbar::createRotationMenu() {
    m_rotateMenu = new QMenu(m_toolbarWidget);

    QAction* action0 = new QAction(tr("0°"), m_rotateMenu);
    action0->setData(0);
    m_rotateMenu->addAction(action0);

    QAction* action90 = new QAction(tr("90°"), m_rotateMenu);
    action90->setData(90);
    m_rotateMenu->addAction(action90);

    QAction* action180 = new QAction(tr("180°"), m_rotateMenu);
    action180->setData(180);
    m_rotateMenu->addAction(action180);

    QAction* action270 = new QAction(tr("270°"), m_rotateMenu);
    action270->setData(270);
    m_rotateMenu->addAction(action270);

    connect(m_rotateMenu, &QMenu::triggered, this, &VideoPlayerToolbar::onRotateAction);
}

void VideoPlayerToolbar::createMirrorMenu() {
    m_mirrorMenu = new QMenu(m_toolbarWidget);

    QAction* actionH = new QAction(tr("Horizontal Mirror"), m_mirrorMenu);
    actionH->setData("horizontal");
    actionH->setCheckable(true);
    m_mirrorMenu->addAction(actionH);

    QAction* actionV = new QAction(tr("Vertical Mirror"), m_mirrorMenu);
    actionV->setData("vertical");
    actionV->setCheckable(true);
    m_mirrorMenu->addAction(actionV);

    connect(m_mirrorMenu, &QMenu::triggered, this, &VideoPlayerToolbar::onMirrorAction);
}

void VideoPlayerToolbar::createZoomMenu() {
    m_zoomMenu = new QMenu(m_toolbarWidget);
    m_zoomMenu->setFixedWidth(180);

    QWidget* zoomWidget = new QWidget(m_zoomMenu);
    QVBoxLayout* zoomLayout = new QVBoxLayout(zoomWidget);
    zoomLayout->setContentsMargins(8, 8, 8, 8);
    zoomLayout->setSpacing(8);

    QHBoxLayout* checkLayout = new QHBoxLayout();
    m_zoomCheckBox = new QCheckBox(tr("Enable Zoom"), zoomWidget);
    connect(m_zoomCheckBox, &QCheckBox::toggled, this, &VideoPlayerToolbar::onZoomToggled);
    checkLayout->addWidget(m_zoomCheckBox);
    checkLayout->addStretch();
    zoomLayout->addLayout(checkLayout);

    QHBoxLayout* spinLayout = new QHBoxLayout();
    spinLayout->addWidget(new QLabel(tr("Scale:"), zoomWidget));

    m_zoomSpinBox = new QDoubleSpinBox(zoomWidget);
    m_zoomSpinBox->setMinimum(1.0);
    m_zoomSpinBox->setMaximum(10.0);
    m_zoomSpinBox->setSingleStep(0.1);
    m_zoomSpinBox->setValue(1.0);
    m_zoomSpinBox->setDecimals(1);
    m_zoomSpinBox->setFixedWidth(80);
    connect(m_zoomSpinBox, QOverload<double>::of(&QDoubleSpinBox::valueChanged),
            this, &VideoPlayerToolbar::onZoomValueChanged);
    spinLayout->addWidget(m_zoomSpinBox);
    spinLayout->addStretch();
    zoomLayout->addLayout(spinLayout);

    QLabel* hintLabel = new QLabel(tr("Use mouse wheel to zoom\nwhen enabled"), zoomWidget);
    hintLabel->setObjectName("ZoomHint");
    hintLabel->setStyleSheet("color: #9e9e9e; font-size: 11px;");
    zoomLayout->addWidget(hintLabel);

    QWidgetAction* widgetAction = new QWidgetAction(m_zoomMenu);
    widgetAction->setDefaultWidget(zoomWidget);
    m_zoomMenu->addAction(widgetAction);
}

void VideoPlayerToolbar::setVideoPlayer(VideoPlayer* player) {
    if (m_videoPlayer) {
        m_videoPlayer->removeEventFilter(this);
    }

    m_videoPlayer = player;

    if (m_videoPlayer) {
        m_videoPlayer->installEventFilter(this);
    }
}

void VideoPlayerToolbar::play(const QString& url) {
    if (m_videoPlayer) m_videoPlayer->play(url);
}

void VideoPlayerToolbar::stop() {
    if (m_videoPlayer) m_videoPlayer->stop();
}

bool VideoPlayerToolbar::isPlaying() const {
    return m_videoPlayer && m_videoPlayer->isPlaying();
}

void VideoPlayerToolbar::onStopClicked() {
    if (m_videoPlayer) {
        m_videoPlayer->stop();
    }
}

void VideoPlayerToolbar::onRotateAction(QAction* action) {
    int angle = action->data().toInt();
    m_rotationAngle = angle;

    if (m_videoPlayer) {
        m_videoPlayer->setRotation(angle);
    }

    LOG_INFO("VideoPlayerToolbar", QString("Rotation set to %1°").arg(angle));
}

void VideoPlayerToolbar::onMirrorAction(QAction* action) {
    QString type = action->data().toString();

    if (type == "horizontal") {
        m_mirrorH = action->isChecked();
    } else if (type == "vertical") {
        m_mirrorV = action->isChecked();
    }

    if (m_videoPlayer) {
        m_videoPlayer->setMirror(m_mirrorH, m_mirrorV);
    }

    LOG_INFO("VideoPlayerToolbar", QString("Mirror H=%1, V=%2").arg(m_mirrorH).arg(m_mirrorV));
}

void VideoPlayerToolbar::onScreenshotClicked() {
    if (!m_videoPlayer) {
        return;
    }

    QString fileName = QFileDialog::getSaveFileName(
        this,
        tr("Save Screenshot"),
        QString("screenshot_%1.png").arg(QDateTime::currentDateTime().toString("yyyyMMdd_hhmmss")),
        tr("Images (*.png *.jpg *.jpeg)")
    );

    if (fileName.isEmpty()) {
        return;
    }

    if (m_videoPlayer->takeScreenshot(fileName)) {
        LOG_INFO("VideoPlayerToolbar", QString("Screenshot saved: %1").arg(fileName));
        QMessageBox::information(this, tr("Success"), tr("Screenshot saved successfully"));
    } else {
        LOG_ERROR("VideoPlayerToolbar", "Screenshot failed");
        QMessageBox::warning(this, tr("Error"), tr("Failed to save screenshot"));
    }
}

void VideoPlayerToolbar::onZoomToggled(bool checked) {
    m_zoomEnabled = checked;

    if (m_videoPlayer) {
        m_videoPlayer->setZoomEnabled(checked);
    }

    refreshIcons();
    LOG_INFO("VideoPlayerToolbar", QString("Zoom enabled: %1").arg(checked));
}

void VideoPlayerToolbar::onZoomValueChanged(double value) {

    if (m_videoPlayer && m_zoomEnabled) {
        QPoint center = QPoint(m_videoPlayer->width() / 2, m_videoPlayer->height() / 2);

    }

    updateZoomLabel();
}

void VideoPlayerToolbar::updateZoomLabel() {
    if (m_zoomLabel) {

    }
}

void VideoPlayerToolbar::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);
    updateCollapseState();
}

int VideoPlayerToolbar::calculateButtonsWidth() const {
    int width = 0;
    for (QWidget* btn : m_primaryButtons) {
        if (btn) {
            width += btn->isHidden() ? btn->sizeHint().width() : btn->width();
            width += m_toolbarLayout->spacing();
        }
    }
    for (QWidget* btn : m_secondaryButtons) {
        if (btn) {
            width += btn->isHidden() ? btn->sizeHint().width() : btn->width();
            width += m_toolbarLayout->spacing();
        }
    }
    if (m_moreBtn) {
        width += m_moreBtn->isHidden() ? m_moreBtn->sizeHint().width() : m_moreBtn->width();
        width += m_toolbarLayout->spacing();
    }
    width += m_toolbarLayout->contentsMargins().left() + m_toolbarLayout->contentsMargins().right();
    return width;
}

void VideoPlayerToolbar::updateCollapseState() {
    if (!m_toolbarWidget) return;

    int availableWidth = m_toolbarWidget->width();
    int buttonsWidth = calculateButtonsWidth();

    if (buttonsWidth > availableWidth && !m_secondaryButtons.isEmpty()) {
        m_moreBtn->show();

        m_collapseMenu->clear();

        if (m_rotateBtn && m_rotateBtn->menu()) {
            QAction* rotateAction = m_collapseMenu->addAction(createColoredIcon(":/icons/ic_toolbar_rotate.svg", m_iconColor), tr("Rotate"));
            rotateAction->setMenu(m_rotateBtn->menu());
        }

        if (m_mirrorBtn && m_mirrorBtn->menu()) {
            QAction* mirrorAction = m_collapseMenu->addAction(createColoredIcon(":/icons/ic_toolbar_mirror_h.svg", m_iconColor), tr("Mirror"));
            mirrorAction->setMenu(m_mirrorBtn->menu());
        }

        if (m_zoomBtn && m_zoomBtn->menu()) {
            QAction* zoomAction = m_collapseMenu->addAction(createColoredIcon(":/icons/ic_toolbar_zoom.svg", m_iconColor), tr("Zoom"));
            zoomAction->setMenu(m_zoomBtn->menu());
        }

        if (m_audioBtn) {
            QAction* audioAction = m_collapseMenu->addAction(createColoredIcon(":/icons/ic_toolbar_audio_on.svg", m_iconColor), tr("Audio On/Off"));
            connect(audioAction, &QAction::triggered, this, [this]() { onAudioClicked(); });
        }

        if (m_mediaRateBtn) {
            QAction* mediaRateAction = m_collapseMenu->addAction(createColoredIcon(":/icons/ic_toolbar_mediainfo.svg", m_iconColor), tr("Media Rate"));
            connect(mediaRateAction, &QAction::triggered, this, [this]() { onMediaRateClicked(); });
        }

        for (QWidget* btn : m_secondaryButtons) {
            if (btn) btn->hide();
        }
    } else {
        m_moreBtn->hide();
        m_collapseMenu->clear();
        for (QWidget* btn : m_secondaryButtons) {
            if (btn) btn->show();
        }
    }
}

QPoint VideoPlayerToolbar::mapToVideoPlayer(const QPoint& pos) {
    if (!m_videoPlayer) {
        return QPoint(0, 0);
    }
    return m_videoPlayer->mapFrom(this, pos);
}

void VideoPlayerToolbar::mousePressEvent(QMouseEvent* event) {
    if (event->button() == Qt::LeftButton) {
        emit clicked();
    }
    QWidget::mousePressEvent(event);
}

void VideoPlayerToolbar::mouseMoveEvent(QMouseEvent* event) {
    QWidget::mouseMoveEvent(event);
}

bool VideoPlayerToolbar::eventFilter(QObject* obj, QEvent* event) {
    if (!m_videoPlayer) {
        return QWidget::eventFilter(obj, event);
    }

    if (obj == m_videoPlayer) {
        if (event->type() == QEvent::Enter) {
            m_toolbarWidget->show();
            updateCollapseState();
            return false;
        } else if (event->type() == QEvent::Leave) {
            QPoint pos = QCursor::pos();
            QRect toolbarRect = m_toolbarWidget->rect().translated(m_toolbarWidget->mapToGlobal(QPoint()));
            if (!toolbarRect.contains(pos)) {
                if (!m_zoomBtn->isChecked()) {
                    m_toolbarWidget->hide();
                }
            }
            return false;
        }
    }

    return QWidget::eventFilter(obj, event);
}

void VideoPlayerToolbar::wheelEvent(QWheelEvent* event) {

        QWidget::wheelEvent(event);

}

void VideoPlayerToolbar::setSelected(bool selected) {
    if (m_videoPlayer) {
        m_videoPlayer->setSelected(selected);
    }
}

void VideoPlayerToolbar::setWatermarkAccount(const QString& account) {
    if (m_videoPlayer) {
        m_videoPlayer->setWatermarkAccount(account);
    }
}

void VideoPlayerToolbar::setDeviceName(const QString& deviceName) {
    if (m_videoPlayer) {
        m_videoPlayer->setDeviceName(deviceName);
    }
}

void VideoPlayerToolbar::toggleFullscreen() {
    if (m_videoPlayer) {
        m_videoPlayer->toggleFullscreen();
    }
}

void VideoPlayerToolbar::onFullscreenClicked() {
    toggleFullscreen();
}

void VideoPlayerToolbar::onAudioClicked() {
    m_audioEnabled = !m_audioEnabled;
    updateAudioIcon();
    if (m_videoPlayer) {
        m_videoPlayer->setAudioEnabled(m_audioEnabled);
    }
}

void VideoPlayerToolbar::updateAudioIcon() {
    if (m_audioBtn) {
        if (m_audioEnabled) {
            m_audioBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_audio_on.svg", m_iconColor));
            m_audioBtn->setToolTip(tr("Audio On"));
        } else {
            m_audioBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_audio_off.svg", m_iconColor));
            m_audioBtn->setToolTip(tr("Audio Off"));
        }
    }
}

void VideoPlayerToolbar::onMediaRateClicked() {
    m_mediaRateEnabled = !m_mediaRateEnabled;
    if (m_mediaRateBtn) {
        m_mediaRateBtn->setChecked(m_mediaRateEnabled);
    }
    if (m_videoPlayer) {
        m_videoPlayer->setMediaRateVisible(m_mediaRateEnabled);
    }
    refreshIcons();
    LOG_INFO("VideoPlayerToolbar", QString("Media rate visible: %1").arg(m_mediaRateEnabled));
}

void VideoPlayerToolbar::onThemeChanged() {
    refreshIcons();
}

void VideoPlayerToolbar::refreshIcons() {
    m_iconColor = ThemeManager::instance()->getColor("text_primary");
    if (!m_iconColor.isValid()) {
        m_iconColor = Qt::white;
    }

    QColor selectedColor = ThemeManager::instance()->getColor("accent");
    if (!selectedColor.isValid()) {
        selectedColor = QColor(0, 120, 212);
    }

    if (m_stopBtn) m_stopBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_stop.svg", m_iconColor));
    if (m_rotateBtn) m_rotateBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_rotate.svg", m_iconColor));
    if (m_mirrorBtn) m_mirrorBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_mirror_h.svg", m_iconColor));
    if (m_screenshotBtn) m_screenshotBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_screenshot.svg", m_iconColor));
    if (m_zoomBtn) {
        QColor zoomColor = m_zoomBtn->isChecked() ? selectedColor : m_iconColor;
        m_zoomBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_zoom.svg", zoomColor));
    }
    if (m_audioBtn) {
        if (m_audioEnabled) {
            m_audioBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_audio_on.svg", m_iconColor));
        } else {
            m_audioBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_audio_off.svg", m_iconColor));
        }
    }
    if (m_mediaRateBtn) {
        QColor mediaRateColor = m_mediaRateBtn->isChecked() ? selectedColor : m_iconColor;
        m_mediaRateBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_mediainfo.svg", mediaRateColor));
    }
    if (m_fullscreenBtn) m_fullscreenBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_fullscreen.svg", m_iconColor));
    if (m_moreBtn) m_moreBtn->setIcon(createColoredIcon(":/icons/ic_toolbar_more.svg", m_iconColor));
}

}
