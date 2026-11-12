#pragma once

#include <QMainWindow>
#include <QString>
#include <QStringList>
#include <QMap>
#include <QStackedWidget>
#include <QVariant>

class QToolBar;
class QStatusBar;
class QLabel;
class QComboBox;

namespace rustcam {

class LeftNavigationBar;
class EventBus;
class ConfigManager;
class PluginManager;

class MainWindow : public QMainWindow {
    Q_OBJECT
public:
    static MainWindow* instance();

    void setupUi();
    void registerPlugins();
    void restoreLayout();
    void saveLayout();

    LeftNavigationBar* navigationBar() const { return m_navBar; }
    QWidget* centralWorkspace() const { return m_workspace; }

    void setStatusMessage(const QString& message, int timeout = 0);
    void setConnectionStatus(bool connected);
    void setAlarmCount(int count);

    void showFullscreenVideo();
    void exitFullscreenVideo();

signals:
    void layoutChanged();

public slots:
    void onNavigationChanged(const QString& id);
    void onPanelVisibilityChanged(bool visible);
    void switchToWorkspace(const QString& workspaceId);

private slots:
    /// 状态栏时钟刷新
    void onClockTick();
    /// 配置变化（例如插件顺序被调整）后同步左侧导航栏
    void onSystemConfigChanged(const QString& key, const QVariant& value);

protected:
    void closeEvent(QCloseEvent* event) override;
    void keyPressEvent(QKeyEvent* event) override;

private:
    explicit MainWindow(QWidget* parent = nullptr);
    ~MainWindow();
    Q_DISABLE_COPY(MainWindow)

    void createToolBar();
    void createStatusBar();
    void createWorkspace();
    QString formatShortcut(Qt::KeyboardModifiers modifiers, int key);
    /// 按配置的插件顺序重新排列左侧导航栏
    void applyPluginOrder();
    /// 读取配置里的插件顺序，配置为空时用默认顺序
    QStringList configuredPluginOrder() const;

    LeftNavigationBar* m_navBar = nullptr;
    QWidget* m_workspace = nullptr;
    QStackedWidget* m_workspaceStack = nullptr;
    QMap<QString, int> m_workspaceIndices;
    QMap<QString, QString> m_pluginShortcuts;
    QStringList m_pluginIds;

    QLabel* m_connectionStatusLabel = nullptr;
    QLabel* m_fpsLabel = nullptr;
    QLabel* m_bitrateLabel = nullptr;
    QLabel* m_gpuLabel = nullptr;
    QLabel* m_cpuLabel = nullptr;
    QLabel* m_timeLabel = nullptr;
    QWidget* m_topRightWidget = nullptr;
    QLabel* m_userLabel = nullptr;
    QComboBox* m_themeCombo = nullptr;

    bool m_fullscreen = false;
};

}
