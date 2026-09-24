#pragma once

#include <QWidget>
#include <QMap>
#include <QString>
#include <QIcon>

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
                          const QString& tooltip);
    void removeNavigationItem(const QString& id);

    void setCurrentItem(const QString& id);
    QString currentItem() const { return m_currentId; }

    void setBadge(const QString& id, int count);
    void setStatusDot(const QString& id, bool on);
    void setIconSize(int size);
    int iconSize() const { return m_iconSize; }

signals:
    void navigationChanged(const QString& id);

public slots:
    void onIconClicked();

protected:
    bool eventFilter(QObject* watched, QEvent* event) override;

private:
    void updateBadgePosition(const QString& id);
    void updateStatusDotPosition(const QString& id);

    QVBoxLayout* m_iconLayout;

    QMap<QString, QToolButton*> m_iconButtons;
    QMap<QString, QLabel*> m_badges;
    QMap<QString, QLabel*> m_statusDots;

    QString m_currentId;
    int m_iconSize = 24;
};

}
