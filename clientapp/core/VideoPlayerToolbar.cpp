#include "VideoPlayerToolbar.h"
#include "IVideoControls.h"
#include "SystemConfig.h"
#include "NotificationWidget.h"
#include "EventBus.h"
#include "ThemeManager.h"
#include "common/Logger.h"
#include <QDir>
#include <QRegularExpression>
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
    , m_controls(nullptr)
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
    if (m_controls && m_controls->controlWidget()) {
        m_controls->controlWidget()->removeEventFilter(this);
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
    // 折叠菜单的动作在构造时创建并连接一次：菜单重建时只重新挂回，不重复连接
    m_collapseRotateAction = new QAction(tr("Rotate"), this);
    m_collapseMirrorAction = new QAction(tr("Mirror"), this);
    m_collapseZoomAction = new QAction(tr("Zoom"), this);
    m_collapseAudioAction = new QAction(tr("Audio On/Off"), this);
    m_collapseMediaRateAction = new QAction(tr("Media Rate"), this);
    connect(m_collapseAudioAction, &QAction::triggered, this, &VideoPlayerToolbar::onAudioClicked);
    connect(m_collapseMediaRateAction, &QAction::triggered, this, &VideoPlayerToolbar::onMediaRateClicked);
    m_collapseActions = {m_collapseRotateAction, m_collapseMirrorAction, m_collapseZoomAction,
                          m_collapseAudioAction, m_collapseMediaRateAction};

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

void VideoPlayerToolbar::setCapabilities(bool screenshot, bool audio, bool fullscreen, bool mediaRate) {
    m_capScreenshot = screenshot;
    m_capAudio = audio;
    m_capFullscreen = fullscreen;
    m_capMediaRate = mediaRate;
    applyCapabilities();
}

void VideoPlayerToolbar::applyCapabilities() {
    if (m_screenshotBtn) m_screenshotBtn->setVisible(m_capScreenshot);
    if (m_audioBtn) m_audioBtn->setVisible(m_capAudio);
    if (m_fullscreenBtn) m_fullscreenBtn->setVisible(m_capFullscreen);
    if (m_mediaRateBtn) m_mediaRateBtn->setVisible(m_capMediaRate);

    m_primaryButtons = {m_stopBtn};
    if (m_capScreenshot) m_primaryButtons.append(m_screenshotBtn);
    if (m_capFullscreen) m_primaryButtons.append(m_fullscreenBtn);

    m_secondaryButtons = {m_rotateBtn, m_mirrorBtn, m_zoomBtn};
    if (m_capAudio) m_secondaryButtons.append(m_audioBtn);
    if (m_capMediaRate) m_secondaryButtons.append(m_mediaRateBtn);
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

void VideoPlayerToolbar::setControls(IVideoControls* controls) {
    if (m_controls && m_controls->controlWidget()) {
        m_controls->controlWidget()->removeEventFilter(this);
    }

    m_controls = controls;

    if (m_controls && m_controls->controlWidget()) {
        m_controls->controlWidget()->installEventFilter(this);
    }
}

void VideoPlayerToolbar::stop() {
    if (m_controls) m_controls->stop();
}

bool VideoPlayerToolbar::isPlaying() const {
    return m_controls && m_controls->isPlaying();
}

void VideoPlayerToolbar::onStopClicked() {
    if (m_controls) {
        m_controls->requestClose();
    }
}

void VideoPlayerToolbar::onRotateAction(QAction* action) {
    int angle = action->data().toInt();
    m_rotationAngle = angle;

    if (m_controls) {
        m_controls->setRotation(angle);
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

    if (m_controls) {
        m_controls->setMirror(m_mirrorH, m_mirrorV);
    }

    LOG_INFO("VideoPlayerToolbar", QString("Mirror H=%1, V=%2").arg(m_mirrorH).arg(m_mirrorV));
}

void VideoPlayerToolbar::onScreenshotClicked() {
    if (!m_controls) {
        return;
    }

    QDir dir(SystemConfig::screenshotDirectory());
    if (!dir.exists() && !dir.mkpath(".")) {
        // 配置目录不可用，回退到工作目录/screenshot
        const QString fallback = SystemConfig::fallbackScreenshotDirectory();
        dir.setPath(fallback);
        if (!dir.exists() && !dir.mkpath(".")) {
            LOG_ERROR("VideoPlayerToolbar", QString("Cannot create screenshot directory: %1")
                .arg(QDir::toNativeSeparators(dir.path())));
            EventBus::instance()->publish(TOPIC_NOTIFICATION_SHOW,
                QSharedPointer<NotificationEventData>(new NotificationEventData(
                    tr("Screenshot failed: cannot create directory"),
                    NotificationEventData::Error)));
            return;
        }
    }

    QString baseName = m_controls->deviceName().trimmed();
    if (baseName.isEmpty()) {
        baseName = "screenshot";
    }
    // 过滤文件系统非法字符
    baseName.replace(QRegularExpression("[\\\\/:*?\"<>|]"), "_");

    const QString fileName = QString("%1_%2.jpeg").arg(
        baseName,
        QDateTime::currentDateTime().toString("yyyy_MM_dd_HH_mm_ss_zzz"));
    const QString filePath = dir.filePath(fileName);

    if (m_controls->takeScreenshot(filePath)) {
        LOG_INFO("VideoPlayerToolbar", QString("Screenshot saved: %1").arg(filePath));
        EventBus::instance()->publish(TOPIC_NOTIFICATION_SHOW,
            QSharedPointer<NotificationEventData>(new NotificationEventData(
                tr("Screenshot saved: %1").arg(QDir::toNativeSeparators(filePath)),
                NotificationEventData::Success)));
    } else {
        LOG_ERROR("VideoPlayerToolbar", QString("Screenshot failed: %1").arg(filePath));
        EventBus::instance()->publish(TOPIC_NOTIFICATION_SHOW,
            QSharedPointer<NotificationEventData>(new NotificationEventData(
                tr("Failed to save screenshot"), NotificationEventData::Error)));
    }
}

void VideoPlayerToolbar::onZoomToggled(bool checked) {
    m_zoomEnabled = checked;

    if (m_controls) {
        m_controls->setZoomEnabled(checked);
    }

    refreshIcons();
    LOG_INFO("VideoPlayerToolbar", QString("Zoom enabled: %1").arg(checked));
}

void VideoPlayerToolbar::onZoomValueChanged(double value) {
Q_UNUSED(value);
    if (m_controls && m_controls->controlWidget() && m_zoomEnabled) {
        QPoint center = QPoint(m_controls->controlWidget()->width() / 2,
                               m_controls->controlWidget()->height() / 2);
        Q_UNUSED(center);
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

        // 先把旧的动作摘下来（不删除），再按当前状态挂回去
        for (QAction* action : m_collapseActions) {
            m_collapseMenu->removeAction(action);
        }

        m_collapseRotateAction->setIcon(createColoredIcon(":/icons/ic_toolbar_rotate.svg", m_iconColor));
        m_collapseMirrorAction->setIcon(createColoredIcon(":/icons/ic_toolbar_mirror_h.svg", m_iconColor));
        m_collapseZoomAction->setIcon(createColoredIcon(":/icons/ic_toolbar_zoom.svg", m_iconColor));
        m_collapseAudioAction->setIcon(createColoredIcon(":/icons/ic_toolbar_audio_on.svg", m_iconColor));
        m_collapseMediaRateAction->setIcon(createColoredIcon(":/icons/ic_toolbar_mediainfo.svg", m_iconColor));

        if (m_rotateBtn && m_rotateBtn->menu()) {
            m_collapseRotateAction->setMenu(m_rotateBtn->menu());
            m_collapseMenu->addAction(m_collapseRotateAction);
        }

        if (m_mirrorBtn && m_mirrorBtn->menu()) {
            m_collapseMirrorAction->setMenu(m_mirrorBtn->menu());
            m_collapseMenu->addAction(m_collapseMirrorAction);
        }

        if (m_zoomBtn && m_zoomBtn->menu()) {
            m_collapseZoomAction->setMenu(m_zoomBtn->menu());
            m_collapseMenu->addAction(m_collapseZoomAction);
        }

        if (m_audioBtn && m_capAudio) {
            m_collapseMenu->addAction(m_collapseAudioAction);
        }

        if (m_mediaRateBtn && m_capMediaRate) {
            m_collapseMenu->addAction(m_collapseMediaRateAction);
        }

        for (QWidget* btn : m_secondaryButtons) {
            if (btn) btn->hide();
        }
    } else {
        m_moreBtn->hide();
        for (QAction* action : m_collapseActions) {
            m_collapseMenu->removeAction(action);
        }
        for (QWidget* btn : m_secondaryButtons) {
            if (btn) btn->show();
        }
    }
}

QPoint VideoPlayerToolbar::mapToVideoPlayer(const QPoint& pos) {
    if (!m_controls || !m_controls->controlWidget()) {
        return QPoint(0, 0);
    }
    return m_controls->controlWidget()->mapFrom(this, pos);
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
    if (!m_controls || !m_controls->controlWidget()) {
        return QWidget::eventFilter(obj, event);
    }

    if (obj == m_controls->controlWidget()) {
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
    if (m_controls) {
        m_controls->setSelected(selected);
    }
}

void VideoPlayerToolbar::setWatermarkAccount(const QString& account) {
    if (m_controls) {
        m_controls->setWatermarkAccount(account);
    }
}

void VideoPlayerToolbar::setDeviceName(const QString& deviceName) {
    if (m_controls) {
        m_controls->setDeviceName(deviceName);
    }
}

void VideoPlayerToolbar::toggleFullscreen() {
    if (m_controls) {
        m_controls->toggleFullscreen();
    }
}
void VideoPlayerToolbar::onFullscreenClicked() {
    toggleFullscreen();
}

void VideoPlayerToolbar::onAudioClicked() {
    m_audioEnabled = !m_audioEnabled;
    updateAudioIcon();
    if (m_controls) {
        m_controls->setAudioEnabled(m_audioEnabled);
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
    if (!m_capMediaRate) {
        return;
    }
    m_mediaRateEnabled = !m_mediaRateEnabled;
    if (m_mediaRateBtn) {
        m_mediaRateBtn->setChecked(m_mediaRateEnabled);
    }
    if (m_controls) {
        m_controls->setMediaRateVisible(m_mediaRateEnabled);
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
