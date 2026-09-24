#include "EventBus.h"
#include "Logger.h"

namespace rustcam {

EventBus::EventBus(QObject* parent)
    : QObject(parent)
    , m_tokenCounter(0)
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

void EventBus::publish(const QString& topic, EventDataPtr data)
{
    int count = 0;
    for (const auto& sub : m_subscriptions) {
        if (sub.topic == topic && sub.handler) {
            count++;
        }
    }
    LOG_INFO("EventBus", QString("Publishing %1 to %2 handlers, data=%3").arg(topic).arg(count).arg(!!data));

    emit eventPublished(topic, data);

    for (const auto& sub : m_subscriptions) {
        if (sub.topic == topic && sub.handler) {
            sub.handler(topic, data);
        }
    }
}

QString EventBus::subscribe(const QString& topic, Handler handler)
{
    Subscription sub;
    sub.token = QString("token_%1").arg(++m_tokenCounter);
    sub.topic = topic;
    sub.handler = handler;
    m_subscriptions.append(sub);
    LOG_DEBUG("EventBus", QString("Subscribed to %1, total: %2").arg(topic).arg(m_subscriptions.size()));
    return sub.token;
}

void EventBus::unsubscribe(const QString& token)
{
    for (int i = 0; i < m_subscriptions.size(); ++i) {
        if (m_subscriptions[i].token == token) {
            m_subscriptions.removeAt(i);
            LOG_DEBUG("EventBus", QString("Unsubscribed token %1, remaining: %2").arg(token).arg(m_subscriptions.size()));
            return;
        }
    }
}

void EventBus::unsubscribeAll()
{
    m_subscriptions.clear();
    LOG_DEBUG("EventBus", "Unsubscribed all handlers");
}

bool EventBus::isSubscribed(const QString& topic) const
{
    for (const auto& sub : m_subscriptions) {
        if (sub.topic == topic) {
            return true;
        }
    }
    return false;
}

QStringList EventBus::subscribedTopics() const
{
    QStringList topics;
    for (const auto& sub : m_subscriptions) {
        if (!topics.contains(sub.topic)) {
            topics.append(sub.topic);
        }
    }
    return topics;
}

}
