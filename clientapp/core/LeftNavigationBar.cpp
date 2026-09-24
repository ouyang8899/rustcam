#include "LeftNavigationBar.h"
#include <QVBoxLayout>
#include <QToolButton>
#include <QLabel>
#include <QEvent>
#include <QMouseEvent>
#include <QStyle>

namespace rustcam {

LeftNavigationBar::LeftNavigationBar(QWidget* parent)
    : QWidget(parent)
{
    setObjectName("LeftNavigationBar");
    setFixedWidth(56);

    QVBoxLayout* mainLayout = new QVBoxLayout(this);
    mainLayout->setContentsMargins(4, 8, 4, 8);
    mainLayout->setSpacing(4);

    QWidget* spacer = new QWidget(this);
    spacer->setSizePolicy(QSizePolicy::Preferred, QSizePolicy::Expanding);
    mainLayout->addWidget(spacer);

    m_iconLayout = mainLayout;

    installEventFilter(this);
}

LeftNavigationBar::~LeftNavigationBar()
{
}

void LeftNavigationBar::addNavigationItem(const QString& id, const QIcon& icon,
                                        const QString& tooltip)
{
    if (m_iconButtons.contains(id)) {
        return;
    }

    QToolButton* btn = new QToolButton(this);
    btn->setObjectName("navIconButton");
    btn->setIcon(icon);
    btn->setIconSize(QSize(m_iconSize, m_iconSize));
    btn->setToolTip(tooltip);
    btn->setCheckable(true);
    btn->setAutoExclusive(true);
    btn->setContextMenuPolicy(Qt::NoContextMenu);

    connect(btn, &QToolButton::clicked, this, &LeftNavigationBar::onIconClicked);

    int insertPos = m_iconLayout->count() - 1;
    m_iconLayout->insertWidget(insertPos, btn);

    m_iconButtons[id] = btn;

    QLabel* badge = new QLabel(this);
    badge->setObjectName("navBadge");
    badge->setVisible(false);
    badge->setAlignment(Qt::AlignCenter);
    m_badges[id] = badge;

    QLabel* statusDot = new QLabel(this);
    statusDot->setObjectName("navStatusDot");
    statusDot->setVisible(false);
    m_statusDots[id] = statusDot;

    updateBadgePosition(id);
    updateStatusDotPosition(id);
}

void LeftNavigationBar::removeNavigationItem(const QString& id)
{
    if (!m_iconButtons.contains(id)) {
        return;
    }

    QToolButton* btn = m_iconButtons.take(id);
    btn->deleteLater();

    if (m_badges.contains(id)) {
        m_badges.take(id)->deleteLater();
    }

    if (m_statusDots.contains(id)) {
        m_statusDots.take(id)->deleteLater();
    }
}

void LeftNavigationBar::setCurrentItem(const QString& id)
{
    if (m_currentId == id) {
        return;
    }

    if (m_currentId.isEmpty() || !m_iconButtons.contains(m_currentId)) {
        if (!m_iconButtons.isEmpty()) {
            QString firstId = m_iconButtons.firstKey();
            m_iconButtons[firstId]->setChecked(true);
        }
    } else {
        if (m_iconButtons.contains(id)) {
            for (auto it = m_iconButtons.begin(); it != m_iconButtons.end(); ++it) {
                it.value()->setChecked(it.key() == id);
            }
        }
    }

    m_currentId = id;
    emit navigationChanged(id);
}

void LeftNavigationBar::onIconClicked()
{
    QToolButton* btn = qobject_cast<QToolButton*>(sender());
    if (!btn) {
        return;
    }

    QString id;
    for (auto it = m_iconButtons.begin(); it != m_iconButtons.end(); ++it) {
        if (it.value() == btn) {
            id = it.key();
            break;
        }
    }

    if (id.isEmpty()) {
        return;
    }

    setCurrentItem(id);
}

void LeftNavigationBar::setBadge(const QString& id, int count)
{
    if (!m_badges.contains(id)) {
        return;
    }

    QLabel* badge = m_badges[id];

    if (count <= 0) {
        badge->setVisible(false);
    } else {
        badge->setVisible(true);
        if (count > 99) {
            badge->setText("99+");
        } else {
            badge->setText(QString::number(count));
        }
    }

    updateBadgePosition(id);
}

void LeftNavigationBar::setStatusDot(const QString& id, bool on)
{
    if (!m_statusDots.contains(id)) {
        return;
    }

    QLabel* dot = m_statusDots[id];
    dot->setVisible(on);
    updateStatusDotPosition(id);
}

void LeftNavigationBar::setIconSize(int size)
{
    m_iconSize = size;
    for (QToolButton* btn : m_iconButtons.values()) {
        btn->setIconSize(QSize(size, size));
    }
}

void LeftNavigationBar::updateBadgePosition(const QString& id)
{
    if (!m_iconButtons.contains(id) || !m_badges.contains(id)) {
        return;
    }

    QToolButton* btn = m_iconButtons[id];
    QLabel* badge = m_badges[id];

    QPoint pos = btn->mapToParent(QPoint(btn->width() - 8, -4));
    badge->move(pos);
}

void LeftNavigationBar::updateStatusDotPosition(const QString& id)
{
    if (!m_iconButtons.contains(id) || !m_statusDots.contains(id)) {
        return;
    }

    QToolButton* btn = m_iconButtons[id];
    QLabel* dot = m_statusDots[id];

    QPoint pos = btn->mapToParent(QPoint(btn->width() - 12, btn->height() - 12));
    dot->move(pos);
}

bool LeftNavigationBar::eventFilter(QObject* watched, QEvent* event)
{
    if (event->type() == QEvent::MouseButtonPress) {
        QMouseEvent* me = static_cast<QMouseEvent*>(event);
        if (me->button() == Qt::MiddleButton) {
            return true;
        }
    }
    return QWidget::eventFilter(watched, event);
}

}
