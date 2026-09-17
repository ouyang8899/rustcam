#include "EventBus.h"
#include <QDebug>

namespace rustcam {

namespace {
const char* TOPIC_device_online = "device/online";
const char* TOPIC_device_offline = "device/offline";
const char* TOPIC_alarm_triggered = "alarm/triggered";
const char* TOPIC_alarm_acknowledged = "alarm/acknowledged";
const char* TOPIC_drone_state = "drone/state";
const char* TOPIC_drone_telemetry = "drone/telemetry";
const char* TOPIC_drone_mission_start = "drone/mission/start";
const char* TOPIC_drone_mission_progress = "drone/mission/progress";
const char* TOPIC_video_stream_request = "video/stream/request";
const char* TOPIC_video_stream_stop = "video/stream/stop";
const char* TOPIC_playback_request = "playback/request";
const char* TOPIC_map_focus = "map/focus";
const char* TOPIC_map_marker_add = "map/marker/add";
const char* TOPIC_layout_change = "layout/change";
const char* TOPIC_permission_changed = "permission/changed";
}

EventBus::EventBus(QObject* parent)
    : QObject(parent)
{
}

EventBus::~EventBus()
{
    m_subscriptions.clear();
}

EventBus* EventBus::instance()
{
    static EventBus inst;
    return &inst;
}

void EventBus::publish(const QString& topic, const QVariantMap& payload)
{
    QList<Subscription> toNotify;
    for (const auto& sub : m_subscriptions) {
        if (sub.topic == topic) {
            toNotify.append(sub);
        }
    }

    emit eventPublished(topic, payload);

    for (const auto& sub : toNotify) {
        if (sub.receiver) {
            QMetaObject::invokeMethod(sub.receiver, sub.slot,
                                     Qt::QueuedConnection,
                                     Q_ARG(QString, topic),
                                     Q_ARG(QVariantMap, payload));
        }
    }
}

void EventBus::subscribe(const QString& topic, QObject* receiver, const char* slot)
{
    Subscription sub;
    sub.topic = topic;
    sub.receiver = receiver;
    sub.slot = slot;

    m_subscriptions.append(sub);

    qDebug() << "EventBus: Subscribed" << topic << "->" << receiver->metaObject()->className() << "::" << slot;
}

void EventBus::unsubscribe(const QString& topic, QObject* receiver)
{
    QList<Subscription> filtered;
    for (const auto& sub : m_subscriptions) {
        if (!(sub.topic == topic && sub.receiver == receiver)) {
            filtered.append(sub);
        }
    }
    m_subscriptions = filtered;
}

void EventBus::unsubscribeAll(QObject* receiver)
{
    QList<Subscription> filtered;
    for (const auto& sub : m_subscriptions) {
        if (sub.receiver != receiver) {
            filtered.append(sub);
        }
    }
    m_subscriptions = filtered;
}

bool EventBus::isSubscribed(const QString& topic, QObject* receiver) const
{
    for (const auto& sub : m_subscriptions) {
        if (sub.topic == topic && sub.receiver == receiver) {
            return true;
        }
    }
    return false;
}

QStringList EventBus::subscribedTopics(QObject* receiver) const
{
    QStringList topics;
    for (const auto& sub : m_subscriptions) {
        if (sub.receiver == receiver) {
            topics.append(sub.topic);
        }
    }
    return topics;
}

}
