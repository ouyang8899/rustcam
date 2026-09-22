#include "EventBus.h"
#include "Logger.h"
#include <QCoreApplication>
#include <QMetaMethod>

namespace rustcam {

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

EventBus::EventBus(QObject* parent)
    : QObject(parent)
{
}

EventBus::~EventBus()
{
    m_subscriptions.clear();
    m_handlerSubscriptions.clear();
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

    LOG_DEBUG("EventBus", QString("Subscribed: %1 -> %2::%3")
               .arg(topic)
               .arg(receiver->metaObject()->className())
               .arg(slot));
}

void EventBus::subscribeHandler(const QString& topic, QObject* receiver, const char* slot)
{
    HandlerSubscription sub;
    sub.topic = topic;
    sub.receiver = receiver;
    sub.slot = slot;

    m_handlerSubscriptions.append(sub);

    LOG_DEBUG("EventBus", QString("Handler subscribed: %1 -> %2::%3")
               .arg(topic)
               .arg(receiver->metaObject()->className())
               .arg(slot));
}

void EventBus::dispatch(const QString& topic, const QVariantMap& payload, Callback callback)
{
    QList<HandlerSubscription> handlers;
    for (const auto& h : m_handlerSubscriptions) {
        if (h.topic == topic) {
            handlers.append(h);
        }
    }

    if (handlers.isEmpty()) {
        LOG_WARNING("EventBus", QString("No handler for dispatch: %1").arg(topic));
        if (callback) {
            QVariantMap result;
            result["_success"] = false;
            result["_error"] = "No handler registered";
            callback(result);
        }
        return;
    }

    const auto& handler = handlers.first();
    if (!handler.receiver) {
        if (callback) {
            QVariantMap result;
            result["_success"] = false;
            result["_error"] = "Handler receiver is null";
            callback(result);
        }
        return;
    }

    QVariantMap result;
    bool invokeResult = QMetaObject::invokeMethod(
        handler.receiver,
        handler.slot,
        Qt::DirectConnection,
        Q_ARG(QString, topic),
        Q_ARG(QVariantMap, payload));

    result["_success"] = invokeResult;
    if (callback) {
        callback(result);
    }

    LOG_DEBUG("EventBus", QString("Dispatched: %1 -> %2::%3")
              .arg(topic)
              .arg(handler.receiver->metaObject()->className())
              .arg(handler.slot));
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

    QList<HandlerSubscription> handlerFiltered;
    for (const auto& h : m_handlerSubscriptions) {
        if (!(h.topic == topic && h.receiver == receiver)) {
            handlerFiltered.append(h);
        }
    }
    m_handlerSubscriptions = handlerFiltered;
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

    QList<HandlerSubscription> handlerFiltered;
    for (const auto& h : m_handlerSubscriptions) {
        if (h.receiver != receiver) {
            handlerFiltered.append(h);
        }
    }
    m_handlerSubscriptions = handlerFiltered;
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
