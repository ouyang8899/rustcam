#pragma once

#include <QWidget>
#include <QMap>
#include <QString>
#include <QIcon>
#include <QSplitter>

class QStackedWidget;
class QToolButton;
class QVBoxLayout;
class QLabel;

namespace rustcam {

class LeftNavigationBar : public QWidget {
    Q_OBJECT
public:
    explicit LeftNavigationBar(QWidget* parent = nullptr);
    ~LeftNavigationBar();

    void addNavigationItem(const QString& id, const QIcon& icon,
                          const QString& tooltip, QWidget* panel,
                          bool isWorkspace = false);
    void removeNavigationItem(const QString& id);

    void setCurrentItem(const QString& id);
    QString currentItem() const { return m_currentId; }

    void setBadge(const QString& id, int count);
    void setStatusDot(const QString& id, bool on);

    void setPanelWidth(int width);
    int panelWidth() const { return m_lastPanelWidth; }

    void setPanelVisible(bool visible);
    bool isPanelVisible() const { return m_panelVisible; }

    void setIconSize(int size);
    int iconSize() const { return m_iconSize; }

signals:
    void navigationChanged(const QString& id);
    void panelVisibilityChanged(bool visible);

public slots:
    void onIconClicked();
    void togglePanel();

protected:
    bool eventFilter(QObject* watched, QEvent* event) override;

private:
    void updateBadgePosition(const QString& id);
    void updateStatusDotPosition(const QString& id);

    QVBoxLayout* m_iconLayout;
    QStackedWidget* m_panelStack;
    QSplitter* m_splitter;

    QMap<QString, QToolButton*> m_iconButtons;
    QMap<QString, QWidget*> m_panels;
    QMap<QString, QLabel*> m_badges;
    QMap<QString, QLabel*> m_statusDots;

    QString m_currentId;
    bool m_panelVisible = true;
    int m_lastPanelWidth = 280;
    int m_iconSize = 24;
};

}
