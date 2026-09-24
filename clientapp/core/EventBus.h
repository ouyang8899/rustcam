#pragma once

#include <QObject>
#include <QString>
#include <QList>
#include <QSharedPointer>
#include <functional>
#include "CoreExport.h"

#define TOPIC_SESSION_LOGGED_IN "session/loggedIn"
#define TOPIC_VIDEO_STREAM_REQUEST "video/stream/request"
#define TOPIC_VIDEO_STREAM_STOP "video/stream/stop"
#define TOPIC_DEVICE_ONLINE "device/online"
#define TOPIC_DEVICE_OFFLINE "device/offline"
#define TOPIC_ALARM_TRIGGERED "alarm/triggered"
#define TOPIC_ALARM_ACKNOWLEDGED "alarm/acknowledged"
#define TOPIC_MAP_FOCUS "map/focus"
#define TOPIC_MAP_MARKER_ADD "map/marker/add"

namespace rustcam {

class RUSTCAM_CORE_API EventResponse {
public:
    virtual ~EventResponse() = default;
};

class RUSTCAM_CORE_API EventData {
public:
    virtual ~EventData() = default;
    QString topic() const { return m_topic; }
    void setCallback(std::function<void(EventResponse*)> cb) { m_callback = cb; }
    void invokeCallback(EventResponse* resp) { if (m_callback) m_callback(resp); }

    protected:
    EventData() : m_callback(nullptr) {}
    explicit EventData(const QString& topic) : m_topic(topic), m_callback(nullptr) {}
    QString m_topic;
    std::function<void(EventResponse*)> m_callback;
};

using EventDataPtr = QSharedPointer<EventData>;

class RUSTCAM_CORE_API EventBus : public QObject {
    Q_OBJECT
public:
    using Handler = std::function<void(const QString& topic, EventDataPtr data)>;

    static EventBus* instance();

    void publish(const QString& topic, EventDataPtr data = nullptr);

    QString subscribe(const QString& topic, Handler handler);
    void unsubscribe(const QString& token);
    void unsubscribeAll();

    bool isSubscribed(const QString& topic) const;
    QStringList subscribedTopics() const;

signals:
    void eventPublished(const QString& topic, EventDataPtr data);

private:
    explicit EventBus(QObject* parent = nullptr);
    ~EventBus();
    Q_DISABLE_COPY(EventBus)

    struct Subscription {
        QString token;
        QString topic;
        Handler handler;
    };

    QList<Subscription> m_subscriptions;
    int m_tokenCounter = 0;
};

}
