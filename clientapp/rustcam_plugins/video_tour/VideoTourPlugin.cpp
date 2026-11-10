#include "VideoTourPlugin.h"

#include "TourApi.h"
#include "TourRunWidget.h"
#include "core/EventBus.h"
#include "core/DataManager.h"
#include "core/NotificationWidget.h"
#include "common/Logger.h"

namespace rustcam {

bool VideoTourPlugin::initialize(EventBus* bus, QObject* cfg)
{
    Q_UNUSED(bus)
    Q_UNUSED(cfg)

    m_api = new TourApi(this);
    m_sessionToken = EventBus::instance()->subscribe(
        TOPIC_SESSION_LOGGED_IN,
        [this](const QString& topic, EventDataPtr data) { onLoggedIn(topic, data); });

    LOG_INFO("VideoTourPlugin", "Initialized");
    return true;
}

void VideoTourPlugin::shutdown()
{
    if (m_runWidget) {
        m_runWidget->stopTask();
    }
    EventBus::instance()->unsubscribe(m_sessionToken);
    m_sessionToken.clear();
    LOG_INFO("VideoTourPlugin", "Shutdown");
}

QWidget* VideoTourPlugin::createWorkspace(QWidget* parent)
{
    if (!m_runWidget) {
        m_runWidget = new TourRunWidget(m_api, parent);
        connect(m_runWidget, &TourRunWidget::notify, this, [](const QString& message, bool isError) {
            EventBus::instance()->publish(
                TOPIC_NOTIFICATION_SHOW,
                QSharedPointer<NotificationEventData>(new NotificationEventData(
                    message, isError ? NotificationEventData::Error : NotificationEventData::Info)));
        });
    }
    return m_runWidget;
}

QWidget* VideoTourPlugin::createSidePanel(QWidget* parent)
{
    Q_UNUSED(parent)
    return nullptr;
}

void VideoTourPlugin::onLoggedIn(const QString& topic, EventDataPtr data)
{
    Q_UNUSED(topic)
    Q_UNUSED(data)

    if (DataManager::instance()->isLoaded()) {
        onDataLoaded(true);
        return;
    }
    connect(DataManager::instance(), &DataManager::loadFinished, this,
            &VideoTourPlugin::onDataLoaded, Qt::UniqueConnection);
    DataManager::instance()->loadAll();
}

void VideoTourPlugin::onDataLoaded(bool success)
{
    disconnect(DataManager::instance(), &DataManager::loadFinished, this,
               &VideoTourPlugin::onDataLoaded);
    if (!success) {
        LOG_ERROR("VideoTourPlugin", "DataManager load failed");
        return;
    }
    if (m_runWidget) {
        m_runWidget->onSessionReady();
    }
}

} // namespace rustcam
