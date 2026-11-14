#include "MainWindow.h"
#include "LeftNavigationBar.h"
#include "EventBus.h"
#include "ConfigManager.h"
#include "ConfigKeys.h"
#include "SystemConfig.h"
#include "PluginManager.h"
#include "PluginOrder.h"
#include "ThemeManager.h"
#include "SessionManager.h"
#include "SystemUsageMonitor.h"
#include "WebSocketClient.h"
#include "Logger.h"
#include "NotificationWidget.h"
#include "interfaces/IPlugin.h"
#include <QJsonDocument>
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

    connect(WebSocketClient::instance(), &WebSocketClient::connectedChanged, this,
            [this](bool connected) {
                m_connectionStatusLabel->setText(connected ? tr("Connected") : tr("Disconnected"));
                m_connectionStatusLabel->setStyleSheet(
                    connected ? "color: #4caf50;" : "color: #f44336;");
            });

    connect(WebSocketClient::instance(), &WebSocketClient::notificationReceived, this,
            [this](const QString& type, const QJsonObject& data) {
                QString message;
                NotificationEventData::Status status = NotificationEventData::Info;

                if (type == "alarm") {
                    message = data.value("message").toString();
                    if (message.isEmpty()) {
                        message = tr("Alarm triggered");
                    }
                    status = NotificationEventData::Warning;
                } else if (type == "device_offline") {
                    message = tr("Device offline: %1")
                                  .arg(data.value("device_id").toVariant().toString());
                    status = NotificationEventData::Error;
                } else if (type == "server_offline") {
                    message = tr("Server offline: %1")
                                  .arg(data.value("server_name").toString());
                    status = NotificationEventData::Error;
                } else {
                    LOG_INFO("MainWindow", QString("WS event %1: %2")
                                               .arg(type)
                                               .arg(QString::fromUtf8(
                                                   QJsonDocument(data).toJson(QJsonDocument::Compact))));
                    return;
                }

                EventBus::instance()->publish(
                    TOPIC_NOTIFICATION_SHOW,
                    EventDataPtr(new NotificationEventData(message, status)));
            });

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

    QMap<QString, IPlugin*> pluginsById;
    QStringList availableIds;
    for (IPlugin* plugin : pm->allPlugins()) {
        pluginsById.insert(plugin->id(), plugin);
        availableIds.append(plugin->id());
    }

    m_pluginIds = orderPluginIds(configuredPluginOrder(), availableIds);

    for (const QString& id : m_pluginIds) {
        IPlugin* plugin = pluginsById.value(id);
        if (!plugin) {
            continue;
        }

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

    connect(SystemConfig::instance(), &SystemConfig::valueChanged,
            this, &MainWindow::onSystemConfigChanged, Qt::UniqueConnection);

    restoreLayout();
}

QStringList MainWindow::configuredPluginOrder() const
{
    QStringList configured = SystemConfig::instance()->getValue(CFG_PLUGIN_ORDER).toStringList();
    if (configured.isEmpty()) {
        configured = defaultPluginIds();
    }
    return configured;
}

void MainWindow::applyPluginOrder()
{
    if (!m_navBar || m_pluginIds.isEmpty()) {
        return;
    }

    QStringList ordered = orderPluginIds(configuredPluginOrder(), m_pluginIds);
    m_pluginIds = ordered;
    m_navBar->setItemOrder(ordered);

    LOG_INFO("MainWindow", QString("Plugin order updated: %1").arg(ordered.join(", ")));
}

void MainWindow::onSystemConfigChanged(const QString& key, const QVariant& value)
{
    Q_UNUSED(value)
    if (key != CFG_PLUGIN_ORDER) {
        return;
    }
    applyPluginOrder();
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
    workspaceLayout->addWidget(m_workspaceStack, 1);

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

    m_cpuLabel = new QLabel("CPU: --");
    statusBar->addPermanentWidget(m_cpuLabel);

    m_memoryLabel = new QLabel(tr("内存: --"));
    statusBar->addPermanentWidget(m_memoryLabel);

    m_gpuBox = new QWidget(statusBar);
    QHBoxLayout* gpuLayout = new QHBoxLayout(m_gpuBox);
    gpuLayout->setContentsMargins(0, 0, 0, 0);
    gpuLayout->setSpacing(8);
    statusBar->addPermanentWidget(m_gpuBox);

    statusBar->addPermanentWidget(new QLabel("|"));

    m_timeLabel = new QLabel;
    statusBar->addPermanentWidget(m_timeLabel);

    QTimer* timer = new QTimer(this);
    connect(timer, &QTimer::timeout, this, &MainWindow::onClockTick);
    timer->start(1000);
    m_timeLabel->setText(QDateTime::currentDateTime().toString("yyyy-MM-dd HH:mm:ss"));

    m_usageMonitor = new SystemUsageMonitor(this);
    connect(m_usageMonitor, &SystemUsageMonitor::usageChanged, this, &MainWindow::onUsageChanged);
}

void MainWindow::onClockTick()
{
    m_timeLabel->setText(QDateTime::currentDateTime().toString("yyyy-MM-dd HH:mm:ss"));
}

void MainWindow::onUsageChanged(const SystemUsage& usage)
{
    m_cpuLabel->setText(usage.cpuValid ? tr("CPU: %1%").arg(qRound(usage.cpuPercent))
                                       : tr("CPU: --"));
    if (usage.memoryValid) {
        const double usedGb = usage.memory.usedBytes / (1024.0 * 1024.0 * 1024.0);
        const double totalGb = usage.memory.totalBytes / (1024.0 * 1024.0 * 1024.0);
        m_memoryLabel->setText(tr("内存: %1% (%2/%3 GB)")
                                   .arg(qRound(usage.memory.percent))
                                   .arg(usedGb, 0, 'f', 1)
                                   .arg(totalGb, 0, 'f', 1));
    } else {
        m_memoryLabel->setText(tr("内存: --"));
    }
    updateGpuLabels(usage.gpus);
}

void MainWindow::updateGpuLabels(const QList<GpuUsage>& gpus)
{
    QHBoxLayout* box = m_gpuBox ? qobject_cast<QHBoxLayout*>(m_gpuBox->layout()) : nullptr;
    if (!box) {
        return;
    }
    while (m_gpuLabels.size() > gpus.size()) {
        QLabel* label = m_gpuLabels.takeLast();
        box->removeWidget(label);
        label->deleteLater();
    }
    while (m_gpuLabels.size() < gpus.size()) {
        QLabel* label = new QLabel(m_gpuBox);
        box->addWidget(label);
        m_gpuLabels.append(label);
    }
    for (int i = 0; i < gpus.size(); ++i) {
        const GpuUsage& gpu = gpus.at(i);
        QString text = tr("%1: 渲染 %2%").arg(gpu.name).arg(qRound(gpu.renderPercent));
        if (gpu.decodeValid) {
            text += tr(" · 解码 %1%").arg(qRound(gpu.decodePercent));
        }
        if (gpu.memoryValid) {
            const double usedGb = gpu.memoryUsedBytes / (1024.0 * 1024.0 * 1024.0);
            const double totalGb = gpu.memoryTotalBytes / (1024.0 * 1024.0 * 1024.0);
            text += tr(" · 显存 %1/%2 GB").arg(usedGb, 0, 'f', 1).arg(totalGb, 0, 'f', 1);
        }
        m_gpuLabels.at(i)->setText(text);
    }
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
