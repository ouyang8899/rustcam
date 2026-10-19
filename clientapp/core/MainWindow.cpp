#include "MainWindow.h"
#include "LeftNavigationBar.h"
#include "EventBus.h"
#include "ConfigManager.h"
#include "PluginManager.h"
#include "ThemeManager.h"
#include "SessionManager.h"
#include "Logger.h"
#include "NotificationWidget.h"
#include "interfaces/IPlugin.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QToolBar>
#include <QStatusBar>
#include <QLabel>
#include <QComboBox>
#include <QDateTime>
#include <QTimer>
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

    NotificationWidget* notifier = new NotificationWidget();
    notifier->hide();
    Q_UNUSED(notifier);

    setupUi();

    EventBus::instance()->subscribe(TOPIC_SESSION_LOGGED_IN, [this](const QString& topic, EventDataPtr) {
        QMetaObject::invokeMethod(this, [this]() {
            QJsonObject userInfo = SessionManager::instance()->userInfo();
            if (!userInfo.isEmpty() && userInfo.contains("username")) {
                m_userLabel->setText(userInfo["username"].toString());
            }
        }, Qt::QueuedConnection);
    });
}

MainWindow::~MainWindow()
{
}

MainWindow* MainWindow::instance()
{
    static MainWindow inst;
    return &inst;
}

void MainWindow::setupUi()
{
    createToolBar();
    createWorkspace();
    createStatusBar();
}

void MainWindow::registerPlugins()
{
    PluginManager* pm = PluginManager::instance();
    for (IPlugin* plugin : pm->allPlugins()) {
        QString id = plugin->id();
        QIcon icon = plugin->icon();
        QString tooltip = plugin->displayName();

        m_navBar->addNavigationItem(id, icon, tooltip);

        if (plugin->isWorkspace()) {
            QWidget* workspace = plugin->createWorkspace(m_workspace);
            if (workspace) {
                int index = m_workspaceStack->addWidget(workspace);
                m_workspaceIndices[id] = index;
            }
        }

        QString shortcut = plugin->shortcut();
        if (!shortcut.isEmpty()) {
            m_pluginShortcuts[shortcut] = id;
        }

        LOG_INFO("MainWindow", QString("Registered plugin: %1 (shortcut: %2)").arg(id).arg(shortcut));
    }

    connect(m_navBar, &LeftNavigationBar::navigationChanged,
            this, &MainWindow::onNavigationChanged);

    restoreLayout();
}

void MainWindow::createToolBar()
{
    QToolBar* toolbar = new QToolBar(tr("Main ToolBar"), this);
    toolbar->setObjectName("mainToolBar");
    toolbar->setMovable(false);
    toolbar->setIconSize(QSize(20, 20));
    addToolBar(Qt::TopToolBarArea, toolbar);

    toolbar->addWidget(new QLabel(" rustcam "));

    QWidget* spacer = new QWidget();
    spacer->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
    toolbar->addWidget(spacer);

    m_topRightWidget = new QWidget(toolbar);
    QHBoxLayout* topRightLayout = new QHBoxLayout(m_topRightWidget);
    topRightLayout->setContentsMargins(8, 2, 8, 2);
    topRightLayout->setSpacing(16);

    m_userLabel = new QLabel("Guest");
    m_userLabel->setObjectName("userLabel");
    topRightLayout->addWidget(m_userLabel);

    QLabel* themeLabel = new QLabel(tr("Theme:"));
    topRightLayout->addWidget(themeLabel);

    m_themeCombo = new QComboBox();
    m_themeCombo->setObjectName("themeCombo");
    m_themeCombo->addItem("Midnight Pro", QVariant((int)ThemeManager::Theme::MidnightPro));
    m_themeCombo->addItem("Arctic Clean", QVariant((int)ThemeManager::Theme::ArcticClean));
    m_themeCombo->addItem("Obsidian Neon", QVariant((int)ThemeManager::Theme::ObsidianNeon));
    topRightLayout->addWidget(m_themeCombo);

    connect(m_themeCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, [this](int index) {
        if (index >= 0) {
            ThemeManager::Theme theme = (ThemeManager::Theme)m_themeCombo->itemData(index).toInt();
            ThemeManager::instance()->setTheme(theme);
        }
    });

    toolbar->addWidget(m_topRightWidget);
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

    QString currentNav = cfg->getValue("ui/nav", "currentItem", "live_video").toString();
    m_navBar->setCurrentItem("");
    m_navBar->setCurrentItem(currentNav);

    restoreGeometry(cfg->getValue("ui", "geometry").toByteArray());
    restoreState(cfg->getValue("ui", "windowState").toByteArray());
}

void MainWindow::saveLayout()
{
    ConfigManager* cfg = ConfigManager::instance();

    if (m_navBar) {
        cfg->setValue("ui/nav", "currentItem", m_navBar->currentItem());
    }

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
    LOG_INFO("MainWindow", QString("onNavigationChanged: id=%1, contains=%2").arg(id).arg(m_workspaceIndices.contains(id)));
    if (m_workspaceIndices.contains(id)) {
        LOG_INFO("MainWindow", QString("Switching to index: %1").arg(m_workspaceIndices[id]));
        m_workspaceStack->setCurrentIndex(m_workspaceIndices[id]);
    }
    emit layoutChanged();
}

void MainWindow::onPanelVisibilityChanged(bool visible)
{
    Q_UNUSED(visible)
    saveLayout();
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

void MainWindow::closeEvent(QCloseEvent* event)
{
    saveLayout();
    PluginManager::instance()->unloadPlugins();
    event->accept();
}

QString MainWindow::formatShortcut(Qt::KeyboardModifiers modifiers, int key)
{
    QStringList parts;
    if (modifiers & Qt::ControlModifier) parts << "Ctrl";
    if (modifiers & Qt::ShiftModifier) parts << "Shift";
    if (modifiers & Qt::AltModifier) parts << "Alt";
    if (modifiers & Qt::MetaModifier) parts << "Meta";

    QString keyText = QKeySequence(key).toString();
    if (!keyText.isEmpty()) {
        parts << keyText;
    }

    return parts.join("+");
}

void MainWindow::keyPressEvent(QKeyEvent* event)
{
    if (event->key() == Qt::Key_Escape && m_fullscreen) {
        exitFullscreenVideo();
        event->accept();
        return;
    }

    QString pressedShortcut = formatShortcut(event->modifiers(), event->key());
    if (m_pluginShortcuts.contains(pressedShortcut)) {
        QString pluginId = m_pluginShortcuts[pressedShortcut];
        switchToWorkspace(pluginId);
        event->accept();
        return;
    }

    QMainWindow::keyPressEvent(event);
}

}
