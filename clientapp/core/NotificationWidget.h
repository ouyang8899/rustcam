#pragma once

#include "EventBus.h"
#include <QWidget>
#include <QLabel>
#include <QTimer>
#include <QPropertyAnimation>

namespace rustcam {

class NotificationEventData : public EventData {
public:
    enum Status {
        Info,
        Success,
        Warning,
        Error
    };

    NotificationEventData(const QString& message, Status status = Info, int durationMs = 3000)
        : EventData(TOPIC_NOTIFICATION_SHOW)
        , message(message)
        , status(status)
        , durationMs(durationMs)
    {}

    QString message;
    Status status;
    int durationMs;
};

class NotificationWidget : public QWidget {
    Q_OBJECT
public:
    explicit NotificationWidget(QWidget* parent = nullptr);
    ~NotificationWidget();

private slots:
    void onNotificationReceived(const QString& topic, EventDataPtr data);
    void onTimeout();
    void onAnimationFinished();

private:
    void reposition();
    void showNotification(const QString& message, NotificationEventData::Status status, int durationMs);
    void updateStyle(NotificationEventData::Status status);
    void animateHide();

    QString m_subscriptionToken;
    QLabel* m_messageLabel;
    QTimer* m_timer;
    QPropertyAnimation* m_animation;
    NotificationEventData::Status m_currentStatus;
};

}
