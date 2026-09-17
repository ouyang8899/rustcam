#include "MainWindow.h"
#include "LeftNavigationBar.h"
#include "EventBus.h"
#include "ConfigManager.h"
#include "PluginManager.h"
#include "ThemeManager.h"
#include "Logger.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QToolBar>
#include <QMenuBar>
#include <QStatusBar>
#include <QLabel>
#include <QComboBox>
#include <QActionGroup>
#include <QKeySequence>
#include <QDateTime>
#include <QTimer>
#include <QMenu>
#include <QToolButton>
#include <QLineEdit>
#include <QCloseEvent>
#include <QKeyEvent>

namespace rustcam {

MainWindow::MainWindow(QWidget* parent)
    : QMainWindow(parent)
{
    setObjectName("MainWindow");
    setWindowTitle("rustcam Client");
    setMinimumSize(1280, 720);
    resize(1600, 900);

    setupUi();
}

MainWindow::~MainWindow()
{
    saveLayout();
}

MainWindow* MainWindow::instance()
{
    static MainWindow inst;
    return &inst;
}

void MainWindow::setupUi()
{
    createMenuBar();
    createToolBar();
    createWorkspace();
    createStatusBar();

    restoreLayout();
}

void MainWindow::createMenuBar()
{
    QMenuBar* menuBar = new QMenuBar(this);
    setMenuBar(menuBar);

    QMenu* fileMenu = menuBar->addMenu(tr("File(&F)"));
    fileMenu->addAction(tr("Import Config..."), this, [](){}, QKeySequence("Ctrl+I"));
    fileMenu->addAction(tr("Export Config..."), this, [](){}, QKeySequence("Ctrl+E"));
    fileMenu->addSeparator();
    fileMenu->addAction(tr("Exit(&X)"), this, &QWidget::close, QKeySequence("Alt+F4"));

    QMenu* viewMenu = menuBar->addMenu(tr("View(&V)"));
    m_layoutMenu = viewMenu->addMenu(tr("Layout(&L)"));
    m_layoutMenu->addAction("1", this, [this](){ switchToWorkspace("live_video"); });
    m_layoutMenu->addAction("4", this, [this](){ switchToWorkspace("live_video"); });
    m_layoutMenu->addAction("9", this, [this](){ switchToWorkspace("live_video"); });
    m_layoutMenu->addAction("16", this, [this](){ switchToWorkspace("live_video"); });

    viewMenu->addSeparator();
    QAction* fullscreenAction = viewMenu->addAction(tr("Fullscreen(&F)"), this, &MainWindow::onFullscreenAction, QKeySequence("F11"));
    fullscreenAction->setCheckable(true);

    m_themeMenu = viewMenu->addMenu(tr("Theme(&T)"));
    QActionGroup* themeGroup = new QActionGroup(this);
    themeGroup->setExclusive(true);

    QStringList themes = ThemeManager::instance()->availableThemes();
    for (int i = 0; i < themes.size(); ++i) {
        QAction* action = m_themeMenu->addAction(themes[i], this, &MainWindow::onThemeActionTriggered);
        action->setCheckable(true);
        action->setData(i);
        themeGroup->addAction(action);
        if (i == 0) {
            action->setChecked(true);
        }
    }

    QMenu* helpMenu = menuBar->addMenu(tr("Help(&H)"));
    helpMenu->addAction(tr("About..."), this, [](){});
    helpMenu->addAction(tr("Check Updates..."), this, [](){});
}

void MainWindow::createToolBar()
{
    QToolBar* toolbar = new QToolBar(tr("Main ToolBar"), this);
    toolbar->setObjectName("mainToolBar");
    toolbar->setMovable(false);
    toolbar->setIconSize(QSize(20, 20));
    addToolBar(Qt::TopToolBarArea, toolbar);

    toolbar->addWidget(new QLabel(" rustcam "));

    QLineEdit* searchBox = new QLineEdit;
    searchBox->setPlaceholderText(tr("Search device..."));
    searchBox->setMaximumWidth(200);
    toolbar->addWidget(searchBox);

    toolbar->addSeparator();

    QLabel* layoutLabel = new QLabel(tr("Layout:"));
    toolbar->addWidget(layoutLabel);

    QComboBox* layoutCombo = new QComboBox;
    layoutCombo->addItems({"1", "4", "6", "8", "9", "13", "16"});
    layoutCombo->setCurrentText("4");
    toolbar->addWidget(layoutCombo);

    toolbar->addSeparator();

    QToolButton* alarmBtn = new QToolButton;
    alarmBtn->setText(tr("Alarm"));
    alarmBtn->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    toolbar->addWidget(alarmBtn);

    QToolButton* themeBtn = new QToolButton;
    themeBtn->setPopupMode(QToolButton::InstantPopup);
    themeBtn->setText(tr("Theme"));
    themeBtn->setMenu(m_themeMenu);
    toolbar->addWidget(themeBtn);

    QToolButton* userBtn = new QToolButton;
    userBtn->setText(tr("User"));
    toolbar->addWidget(userBtn);
}

void MainWindow::createWorkspace()
{
    QWidget* central = new QWidget(this);
    central->setObjectName("centralArea");
    setCentralWidget(central);

    QHBoxLayout* layout = new QHBoxLayout(central);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_navBar = new LeftNavigationBar(central);
    layout->addWidget(m_navBar);

    m_workspace = new QWidget(central);
    m_workspace->setObjectName("workspaceArea");

    QVBoxLayout* workspaceLayout = new QVBoxLayout(m_workspace);
    workspaceLayout->setContentsMargins(4, 4, 4, 4);

    m_workspaceStack = new QStackedWidget(m_workspace);
    workspaceLayout->addWidget(m_workspaceStack);

    layout->addWidget(m_workspace, 1);
}

void MainWindow::createStatusBar()
{
    QStatusBar* statusBar = new QStatusBar(this);
    setStatusBar(statusBar);

    m_connectionStatusLabel = new QLabel(tr("Disconnected"));
    m_connectionStatusLabel->setObjectName("statusConnection");
    statusBar->addPermanentWidget(m_connectionStatusLabel);

    statusBar->addPermanentWidget(new QLabel("|"));

    m_fpsLabel = new QLabel("0 fps");
    statusBar->addPermanentWidget(m_fpsLabel);

    m_bitrateLabel = new QLabel("0 Kbps");
    statusBar->addPermanentWidget(m_bitrateLabel);

    m_gpuLabel = new QLabel("GPU: 0%");
    statusBar->addPermanentWidget(m_gpuLabel);

    m_cpuLabel = new QLabel("CPU: 0%");
    statusBar->addPermanentWidget(m_cpuLabel);

    statusBar->addPermanentWidget(new QLabel("|"));

    m_timeLabel = new QLabel;
    statusBar->addPermanentWidget(m_timeLabel);

    QTimer* timer = new QTimer(this);
    connect(timer, &QTimer::timeout, [this]() {
        m_timeLabel->setText(QDateTime::currentDateTime().toString("yyyy-MM-dd HH:mm:ss"));
    });
    timer->start(1000);
    m_timeLabel->setText(QDateTime::currentDateTime().toString("yyyy-MM-dd HH:mm:ss"));
}

void MainWindow::restoreLayout()
{
    ConfigManager* cfg = ConfigManager::instance();

    QString currentNav = cfg->getValue("ui/nav", "currentItem", "device_tree").toString();
    int panelWidth = cfg->getValue("ui/nav", "panelWidth", 280).toInt();
    bool panelVisible = cfg->getValue("ui/nav", "panelVisible", true).toBool();

    m_navBar->setCurrentItem(currentNav);
    m_navBar->setPanelWidth(panelWidth);
    m_navBar->setPanelVisible(panelVisible);

    restoreGeometry(cfg->getValue("ui", "geometry").toByteArray());
    restoreState(cfg->getValue("ui", "windowState").toByteArray());
}

void MainWindow::saveLayout()
{
    ConfigManager* cfg = ConfigManager::instance();

    cfg->setValue("ui/nav", "currentItem", m_navBar->currentItem());
    cfg->setValue("ui/nav", "panelWidth", m_navBar->panelWidth());
    cfg->setValue("ui/nav", "panelVisible", m_navBar->isPanelVisible());

    cfg->setValue("ui", "geometry", saveGeometry());
    cfg->setValue("ui", "windowState", saveState());

    cfg->sync();
}

void MainWindow::setStatusMessage(const QString& message, int timeout)
{
    statusBar()->showMessage(message, timeout);
}

void MainWindow::setConnectionStatus(bool connected)
{
    if (connected) {
        m_connectionStatusLabel->setText(tr("Connected"));
        m_connectionStatusLabel->setStyleSheet("color: #4caf50;");
    } else {
        m_connectionStatusLabel->setText(tr("Disconnected"));
        m_connectionStatusLabel->setStyleSheet("color: #f44336;");
    }
}

void MainWindow::setAlarmCount(int count)
{
    m_navBar->setBadge("alarm", count);
}

void MainWindow::switchToWorkspace(const QString& workspaceId)
{
    m_navBar->setCurrentItem(workspaceId);
}

void MainWindow::onNavigationChanged(const QString& id)
{
    emit layoutChanged();
}

void MainWindow::onPanelVisibilityChanged(bool visible)
{
    Q_UNUSED(visible)
    saveLayout();
}

void MainWindow::onThemeActionTriggered(bool checked)
{
    Q_UNUSED(checked)
    auto* action = qobject_cast<QAction*>(sender());
    if (action) {
        int themeIndex = action->data().toInt();
        ThemeManager::instance()->setTheme(static_cast<ThemeManager::Theme>(themeIndex));
    }
}

void MainWindow::onFullscreenAction()
{
    if (m_fullscreen) {
        exitFullscreenVideo();
    } else {
        showFullscreenVideo();
    }
}

void MainWindow::showFullscreenVideo()
{
    showFullScreen();
    m_fullscreen = true;
}

void MainWindow::exitFullscreenVideo()
{
    showNormal();
    m_fullscreen = false;
}

void MainWindow::onExitFullscreenAction()
{
    exitFullscreenVideo();
}

void MainWindow::closeEvent(QCloseEvent* event)
{
    saveLayout();
    PluginManager::instance()->unloadPlugins();
    event->accept();
}

void MainWindow::keyPressEvent(QKeyEvent* event)
{
    if (event->key() == Qt::Key_Escape && m_fullscreen) {
        exitFullscreenVideo();
        event->accept();
        return;
    }

    if (event->modifiers() & Qt::ControlModifier) {
        switch (event->key()) {
            case Qt::Key_1:
            switchToWorkspace("device_tree");
            break;
            case Qt::Key_2:
            switchToWorkspace("map");
            break;
            case Qt::Key_3:
            switchToWorkspace("alarm");
            break;
            case Qt::Key_4:
            switchToWorkspace("drone");
            break;
            case Qt::Key_5:
            switchToWorkspace("playback");
            break;
            case Qt::Key_6:
            switchToWorkspace("dashboard");
            break;
            case Qt::Key_B:
            if (event->modifiers() & Qt::ShiftModifier) {
                m_workspace->hide();
            } else {
                m_navBar->togglePanel();
            }
            break;
        }
    }

    QMainWindow::keyPressEvent(event);
}

}
