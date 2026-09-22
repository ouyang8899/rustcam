#pragma once

#include <QObject>
#include <QVariantMap>
#include <QString>
#include <QList>
#include <functional>

namespace rustcam {

class EventBus : public QObject {
    Q_OBJECT
public:
    using Callback = std::function<void(const QVariantMap& result)>;

    static EventBus* instance();

    void publish(const QString& topic, const QVariantMap& payload = {});

    void subscribe(const QString& topic, QObject* receiver, const char* slot);
    void unsubscribe(const QString& topic, QObject* receiver);
    void unsubscribeAll(QObject* receiver);

    void subscribeHandler(const QString& topic, QObject* receiver, const char* slot);
    void dispatch(const QString& topic, const QVariantMap& payload, Callback callback);

    bool isSubscribed(const QString& topic, QObject* receiver) const;
    QStringList subscribedTopics(QObject* receiver) const;

signals:
    void eventPublished(const QString& topic, const QVariantMap& payload);

private:
    explicit EventBus(QObject* parent = nullptr);
    ~EventBus();
    Q_DISABLE_COPY(EventBus)

    struct Subscription {
        QString topic;
        QObject* receiver;
        const char* slot;
    };

    struct HandlerSubscription {
        QString topic;
        QObject* receiver;
        const char* slot;
    };

    QList<Subscription> m_subscriptions;
    QList<HandlerSubscription> m_handlerSubscriptions;
};

}
