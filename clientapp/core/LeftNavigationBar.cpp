#include "LeftNavigationBar.h"
#include "common/Logger.h"
#include <QVBoxLayout>
#include <QToolButton>
#include <QLabel>
#include <QEvent>
#include <QMouseEvent>
#include <QStyle>
#include <QSignalMapper>

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

    // 按钮 -> 导航 id：点击时由它把 id 传出来（不用 sender() 反查）
    m_idMapper = new QSignalMapper(this);
    connect(m_idMapper, QOverload<const QString&>::of(&QSignalMapper::mapped),
            this, &LeftNavigationBar::onIconClicked);

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

    connect(btn, &QToolButton::clicked, m_idMapper, QOverload<>::of(&QSignalMapper::map));
    m_idMapper->setMapping(btn, id);

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
    if (m_idMapper) {
        m_idMapper->removeMappings(btn);
    }
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
    LOG_INFO("LeftNavigationBar", QString("setCurrentItem called: id=%1, m_currentId=%2").arg(id).arg(m_currentId));
    if (!id.isEmpty() && m_currentId == id) {
        LOG_INFO("LeftNavigationBar", "setCurrentItem early return - same id");
        return;
    }

    for (auto it = m_iconButtons.begin(); it != m_iconButtons.end(); ++it) {
        it.value()->setChecked(it.key() == id);
    }

    m_currentId = id;
    if (!id.isEmpty()) {
        LOG_INFO("LeftNavigationBar", QString("setCurrentItem emitting: %1").arg(id));
        emit navigationChanged(id);
    }
}

void LeftNavigationBar::onIconClicked(const QString& id)
{
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
            badge->setText(tr("99+"));
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
