#pragma once

#include <QMainWindow>
#include <QString>
#include <QMap>
#include <QStackedWidget>

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

    LeftNavigationBar* m_navBar = nullptr;
    QWidget* m_workspace = nullptr;
    QStackedWidget* m_workspaceStack = nullptr;
    QMap<QString, int> m_workspaceIndices;

    QLabel* m_connectionStatusLabel = nullptr;
    QLabel* m_fpsLabel = nullptr;
    QLabel* m_bitrateLabel = nullptr;
    QLabel* m_gpuLabel = nullptr;
    QLabel* m_cpuLabel = nullptr;
    QLabel* m_timeLabel = nullptr;

    bool m_fullscreen = false;
};

}
