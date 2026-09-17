#pragma once

#include <QObject>
#include <QString>

class QApplication;

namespace rustcam {

class App : public QObject {
    Q_OBJECT
public:
    static App* instance();

    bool initialize();
    void run();
    void quit();

    QString version() const { return "1.0.0"; }
    QString buildNumber() const { return "1"; }

    void setApplicationName(const QString& name);
    void setOrganizationName(const QString& name);

    bool eventFilter(QObject* watched, QEvent* event) override;

signals:
    void initialized();
    void aboutToQuit();

private:
    explicit App(QObject* parent = nullptr);
    ~App();
    Q_DISABLE_COPY(App)

    bool parseArguments();
    void setupEnvironment();
    void setupLogging();
    void loadPlugins();

    QApplication* m_app = nullptr;
    bool m_initialized = false;
};

}
