#pragma once

#include <QMainWindow>
#include <QString>
#include <QMap>
#include <QStackedWidget>

class QToolBar;
class QMenuBar;
class QStatusBar;
class QLabel;
class QComboBox;
class QAction;

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
    void themeChangeRequested(int themeIndex);

public slots:
    void onNavigationChanged(const QString& id);
    void onPanelVisibilityChanged(bool visible);
    void switchToWorkspace(const QString& workspaceId);

protected:
    void closeEvent(QCloseEvent* event) override;
    void keyPressEvent(QKeyEvent* event) override;

private slots:
    void onThemeActionTriggered(bool checked);
    void onFullscreenAction();
    void onExitFullscreenAction();

private:
    explicit MainWindow(QWidget* parent = nullptr);
    ~MainWindow();
    Q_DISABLE_COPY(MainWindow)

    void createMenuBar();
    void createToolBar();
    void createStatusBar();
    void createWorkspace();

    void loadStylesheet(const QString& themeName);

    LeftNavigationBar* m_navBar = nullptr;
    QWidget* m_workspace = nullptr;
    QStackedWidget* m_workspaceStack = nullptr;

    QLabel* m_connectionStatusLabel = nullptr;
    QLabel* m_fpsLabel = nullptr;
    QLabel* m_bitrateLabel = nullptr;
    QLabel* m_gpuLabel = nullptr;
    QLabel* m_cpuLabel = nullptr;
    QLabel* m_timeLabel = nullptr;

    QMenu* m_themeMenu = nullptr;
    QMenu* m_layoutMenu = nullptr;

    bool m_fullscreen = false;
};

}
