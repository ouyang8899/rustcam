#include "NotificationWidget.h"
#include <QVBoxLayout>
#include <QGuiApplication>
#include <QScreen>

namespace rustcam {

NotificationWidget::NotificationWidget(QWidget* parent)
    : QWidget(parent, Qt::Tool | Qt::FramelessWindowHint | Qt::NoDropShadowWindowHint)
    , m_timer(new QTimer(this))
    , m_animation(new QPropertyAnimation(this, "windowOpacity", this))
    , m_currentStatus(NotificationEventData::Info)
{
    setAttribute(Qt::WA_TranslucentBackground);
    setAttribute(Qt::WA_ShowWithoutActivating);
    setFocusPolicy(Qt::NoFocus);

    QVBoxLayout* layout = new QVBoxLayout(this);
    layout->setContentsMargins(12, 8, 12, 8);
    layout->setSpacing(4);

    m_messageLabel = new QLabel(this);
    m_messageLabel->setObjectName("NotificationLabel");
    m_messageLabel->setAlignment(Qt::AlignCenter);
    m_messageLabel->setWordWrap(true);
    layout->addWidget(m_messageLabel);

    m_timer->setSingleShot(true);
    connect(m_timer, &QTimer::timeout, this, &NotificationWidget::onTimeout);

    m_animation->setDuration(250);
    m_animation->setEasingCurve(QEasingCurve::OutCubic);
    connect(m_animation, &QPropertyAnimation::finished, this, &NotificationWidget::onAnimationFinished);

    setFixedWidth(320);
    setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Preferred);

    reposition();

    m_subscriptionToken = EventBus::instance()->subscribe(TOPIC_NOTIFICATION_SHOW, [this](const QString& topic, EventDataPtr data) {
        Q_UNUSED(topic)
        auto notifData = qSharedPointerCast<NotificationEventData>(data);
        if (notifData) {
            showNotification(notifData->message, notifData->status, notifData->durationMs);
        }
    });
}

NotificationWidget::~NotificationWidget() {
    EventBus::instance()->unsubscribe(m_subscriptionToken);
}

void NotificationWidget::reposition() {
    QScreen* screen = QGuiApplication::primaryScreen();
    if (!screen) return;

    QRect screenGeom = screen->availableGeometry();
    int x = screenGeom.right() - QWidget::width() - 16;
    int y = screenGeom.bottom() - QWidget::height() - 16;
    move(x, y);
}

void NotificationWidget::showNotification(const QString& message, NotificationEventData::Status status, int durationMs) {
    m_timer->stop();
    m_animation->stop();

    m_messageLabel->setText(message);
    m_currentStatus = status;
    updateStyle(status);

    reposition();
    show();
    raise();

    m_timer->start(durationMs);
}

void NotificationWidget::updateStyle(NotificationEventData::Status status) {
    QString color;
    QString bgColor;
    switch (status) {
    case NotificationEventData::Success:
        color = "#4CAF50";
        bgColor = "#E8F5E9";
        break;
    case NotificationEventData::Warning:
        color = "#FF9800";
        bgColor = "#FFF3E0";
        break;
    case NotificationEventData::Error:
        color = "#F44336";
        bgColor = "#FFEBEE";
        break;
    default:
        color = "#2196F3";
        bgColor = "#E3F2FD";
    }

    QString style = QString(
        "NotificationWidget {"
        "   background-color: %1;"
        "   border-radius: 6px;"
        "   border: 1px solid %2;"
        "}"
        "QLabel {"
        "   color: %2;"
        "   font-size: 13px;"
        "   background-color: transparent;"
        "}"
    ).arg(bgColor, color);

    setStyleSheet(style);
}

void NotificationWidget::onNotificationReceived(const QString& topic, EventDataPtr data) {
    auto notifData = qSharedPointerCast<NotificationEventData>(data);
    if (notifData) {
        showNotification(notifData->message, notifData->status, notifData->durationMs);
    }
}

void NotificationWidget::onTimeout() {
    animateHide();
}

void NotificationWidget::animateHide() {
    if (!isVisible()) return;

    m_animation->setStartValue(1.0);
    m_animation->setEndValue(0.0);
    m_animation->start();
}

void NotificationWidget::onAnimationFinished() {
    if (m_animation->direction() == QPropertyAnimation::Backward) {
        hide();
        m_animation->setDirection(QPropertyAnimation::Forward);
    }
}

}
