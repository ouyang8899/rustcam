#include "LeftNavigationBar.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QStackedWidget>
#include <QSplitter>
#include <QToolButton>
#include <QLabel>
#include <QStyle>
#include <QEvent>
#include <QMouseEvent>
#include <QToolTip>

namespace rustcam {

LeftNavigationBar::LeftNavigationBar(QWidget* parent)
    : QWidget(parent)
{
    setObjectName("LeftNavigationBar");

    QHBoxLayout* mainLayout = new QHBoxLayout(this);
    mainLayout->setContentsMargins(4, 8, 4, 8);
    mainLayout->setSpacing(0);

    QWidget* iconBar = new QWidget(this);
    iconBar->setObjectName("navIconBar");
    iconBar->setFixedWidth(56);
    QVBoxLayout* iconBarLayout = new QVBoxLayout(iconBar);
    iconBarLayout->setContentsMargins(0, 0, 0, 0);
    iconBarLayout->setSpacing(4);

    QWidget* spacer = new QWidget(this);
    spacer->setSizePolicy(QSizePolicy::Preferred, QSizePolicy::Expanding);
    iconBarLayout->addWidget(spacer);

    m_panelStack = new QStackedWidget(this);
    m_panelStack->setObjectName("navPanelStack");
    m_panelStack->setFixedWidth(m_lastPanelWidth);

    m_splitter = new QSplitter(Qt::Horizontal, this);
    m_splitter->setChildrenCollapsible(false);
    m_splitter->setHandleWidth(1);
    m_splitter->addWidget(iconBar);
    m_splitter->addWidget(m_panelStack);
    m_splitter->setStretchFactor(0, 0);
    m_splitter->setStretchFactor(1, 1);

    mainLayout->addWidget(m_splitter);

    iconBar->installEventFilter(this);
}

LeftNavigationBar::~LeftNavigationBar()
{
}

void LeftNavigationBar::addNavigationItem(const QString& id, const QIcon& icon,
                                          const QString& tooltip, QWidget* panel,
                                          bool isWorkspace)
{
    Q_UNUSED(isWorkspace)

    if (m_iconButtons.contains(id)) {
        return;
    }

    QWidget* iconBar = m_splitter->widget(0);
    QVBoxLayout* iconBarLayout = qobject_cast<QVBoxLayout*>(iconBar->layout());

    QToolButton* btn = new QToolButton(this);
    btn->setObjectName("navIconButton");
    btn->setIcon(icon);
    btn->setIconSize(QSize(m_iconSize, m_iconSize));
    btn->setToolTip(tooltip);
    btn->setCheckable(true);
    btn->setAutoExclusive(true);
    btn->setContextMenuPolicy(Qt::NoContextMenu);

    connect(btn, &QToolButton::clicked, this, &LeftNavigationBar::onIconClicked);

    int insertPos = iconBarLayout->count() - 1;
    iconBarLayout->insertWidget(insertPos, btn);

    m_iconButtons[id] = btn;

    if (panel) {
        panel->setParent(m_panelStack);
        m_panelStack->addWidget(panel);
        m_panels[id] = panel;
    }

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

    if (m_panels.contains(id)) {
        QWidget* panel = m_panels.take(id);
        m_panelStack->removeWidget(panel);
        panel->deleteLater();
    }

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
            if (m_panels.contains(firstId)) {
                m_panelStack->setCurrentWidget(m_panels[firstId]);
            }
        }
    } else {
        if (m_iconButtons.contains(id)) {
            for (auto it = m_iconButtons.begin(); it != m_iconButtons.end(); ++it) {
                it.value()->setChecked(it.key() == id);
            }

            if (m_panels.contains(id)) {
                m_panelStack->setCurrentWidget(m_panels[id]);
            }

            if (!m_panelVisible && id != m_currentId) {
                setPanelVisible(true);
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

    if (m_currentId == id && m_panelVisible) {
        setPanelVisible(false);
    } else {
        setPanelVisible(true);
        setCurrentItem(id);
    }
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

void LeftNavigationBar::setPanelWidth(int width)
{
    m_lastPanelWidth = width;
    m_panelStack->setFixedWidth(width);
}

void LeftNavigationBar::setPanelVisible(bool visible)
{
    if (m_panelVisible == visible) {
        return;
    }

    m_panelVisible = visible;
    m_panelStack->setVisible(visible);
    emit panelVisibilityChanged(visible);
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
            QWidget* iconBar = m_splitter->widget(0);
            if (watched == iconBar) {
                togglePanel();
                return true;
            }
        }
    }
    return QWidget::eventFilter(watched, event);
}

void LeftNavigationBar::togglePanel()
{
    setPanelVisible(!m_panelVisible);
}

}
