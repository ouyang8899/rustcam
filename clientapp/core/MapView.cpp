#include "MapView.h"

#ifdef RUSTCAM_WITH_WEBENGINE

#include <QWebEngineSettings>
#include <QWebEnginePage>
#include <QGridLayout>
#include <QTimer>
#include "common/Logger.h"

namespace rustcam {

class MapWebPage : public QWebEnginePage {
    Q_OBJECT
public:
    MapWebPage(QWebEngineProfile* profile, QObject* parent = nullptr)
        : QWebEnginePage(profile, parent) {}

    bool acceptNavigationRequest(const QUrl& url, NavigationType, bool) override {
        if (url.scheme() == "qrc") {
            return true;
        }
        return false;
    }
};

MapView::MapView(QWidget* parent)
    : QWidget(parent)
    , m_webView(nullptr)
    , m_jsReady(false)
{
    QGridLayout* layout = new QGridLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_webView = new QWebEngineView(this);

    QWebEngineSettings* settings = m_webView->settings();
    settings->setAttribute(QWebEngineSettings::LocalContentCanAccessRemoteUrls, true);
    settings->setAttribute(QWebEngineSettings::JavascriptEnabled, true);
    settings->setAttribute(QWebEngineSettings::LocalStorageEnabled, true);

    MapWebPage* page = new MapWebPage(m_webView->page()->profile(), this);
    m_webView->setPage(page);

    layout->addWidget(m_webView);

    m_webView->setUrl(QUrl("qrc:/resources/web/map.html"));

    connect(m_webView, &QWebEngineView::loadFinished, this, [this](bool ok) {
        if (ok) {
            m_webView->page()->runJavaScript("typeof mapReady !== 'undefined'", [this](const QVariant& result) {
                if (result.toBool()) {
                    m_jsReady = true;
                    emit mapReady();
                    for (const auto& pair : m_pendingCalls) {
                        runJavaScript(pair.second);
                    }
                    m_pendingCalls.clear();
                } else {
                    QTimer::singleShot(100, this, &MapView::onJavaScriptReady);
                }
            });
        } else {
            LOG_ERROR("MapView", "Failed to load map HTML");
        }
    });
}

MapView::~MapView() = default;

void MapView::initialize(double centerLon, double centerLat, int zoom) {
    QString script = QString("initMap(%1, %2, %3)").arg(centerLon).arg(centerLat).arg(zoom);
    runJavaScript(script);
}

void MapView::setCenter(double lon, double lat, int zoom) {
    if (zoom < 0) {
        QString script = QString("setCenter(%1, %2)").arg(lon).arg(lat);
        runJavaScript(script);
    } else {
        QString script = QString("setCenter(%1, %2, %3)").arg(lon).arg(lat).arg(zoom);
        runJavaScript(script);
    }
}

void MapView::addMarker(const MapMarker& marker) {
    QString script = QString("addMarker('%1', %2, %3, '%4', '%5', %6, '%7')")
        .arg(marker.id)
        .arg(marker.lon)
        .arg(marker.lat)
        .arg(marker.name)
        .arg(marker.type)
        .arg(marker.online ? "true" : "false")
        .arg(marker.status);
    runJavaScript(script);
}

void MapView::removeMarker(const QString& markerId) {
    QString script = QString("removeMarker('%1')").arg(markerId);
    runJavaScript(script);
}

void MapView::updateMarker(const MapMarker& marker) {
    QString script = QString("updateMarker('%1', %2, %3, '%4', '%5', %6, '%7')")
        .arg(marker.id)
        .arg(marker.lon)
        .arg(marker.lat)
        .arg(marker.name)
        .arg(marker.type)
        .arg(marker.online ? "true" : "false")
        .arg(marker.status);
    runJavaScript(script);
}

void MapView::clearMarkers() {
    runJavaScript("clearMarkers()");
}

void MapView::zoomIn() {
    runJavaScript("zoomIn()");
}

void MapView::zoomOut() {
    runJavaScript("zoomOut()");
}

void MapView::setZoom(int zoom) {
    QString script = QString("setZoom(%1)").arg(zoom);
    runJavaScript(script);
}

void MapView::onJavaScriptReady() {
    m_webView->page()->runJavaScript("typeof mapReady !== 'undefined'", [this](const QVariant& result) {
        if (result.toBool()) {
            m_jsReady = true;
            emit mapReady();
            for (const auto& pair : m_pendingCalls) {
                runJavaScript(pair.second);
            }
            m_pendingCalls.clear();
        } else {
            QTimer::singleShot(100, this, &MapView::onJavaScriptReady);
        }
    });
}

void MapView::runJavaScript(const QString& script) {
    if (!m_jsReady) {
        m_pendingCalls.append(qMakePair(QString(), script));
        return;
    }
    m_webView->page()->runJavaScript(script);
}

void MapView::handleJavaScriptResult(QVariant result) {
    Q_UNUSED(result)
}

}

#include "MapView.moc"
#endif
