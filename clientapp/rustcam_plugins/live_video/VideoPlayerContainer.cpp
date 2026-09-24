#include "VideoPlayerContainer.h"
#include "NavigationBar.h"
#include "core/VideoPlayer.h"
#include <QVBoxLayout>
#include <QDebug>
#include <QtConcurrent>
#include <QFutureWatcher>

namespace rustcam {

VideoPlayerContainer::VideoPlayerContainer(QWidget* parent)
    : QWidget(parent)
    , m_videoArea(nullptr)
    , m_navBar(nullptr)
{
    QVBoxLayout* layout = new QVBoxLayout(this);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->setSpacing(0);

    m_videoArea = new QWidget(this);
    m_videoArea->setObjectName("videoArea");
    m_videoArea->setAcceptDrops(true);

    m_navBar = new NavigationBar(this);
    m_navBar->setObjectName("navBar");

    layout->addWidget(m_videoArea, 1);
    layout->addWidget(m_navBar, 0);

    connect(m_navBar, &NavigationBar::layoutSelected, this, &VideoPlayerContainer::onLayoutSelected);

    qDebug() << "VideoPlayerContainer created, size:" << size();
}

VideoPlayerContainer::~VideoPlayerContainer() {
    VideoWidgetManager::instance().clearLayout();
}

void VideoPlayerContainer::initialize()
{
    loadLayouts();
}

void VideoPlayerContainer::loadLayouts() {
    qDebug() << "loadLayouts called at" << QDateTime::currentDateTime().toString("hh:mm:ss.zzz");
    QFuture<QPair<bool, QJsonObject>> future = QtConcurrent::run([]() {
        QJsonObject response;
        QString error;
        bool success = ServerApi::instance()->getSync("/api/v1/layouts", response, error);
        Q_UNUSED(error)
        return QPair<bool, QJsonObject>(success, response);
    });

    QFutureWatcher<QPair<bool, QJsonObject>>* watcher = new QFutureWatcher<QPair<bool, QJsonObject>>(this);
    connect(watcher, &QFutureWatcher<QPair<bool, QJsonObject>>::finished, [this, watcher]() {
        QPair<bool, QJsonObject> result = watcher->result();
        onLayoutsReceived(result.first, result.second, QString());
        watcher->deleteLater();
    });
    watcher->setFuture(future);
}

void VideoPlayerContainer::onLayoutsReceived(bool success, const QJsonObject& response, const QString& error) {
    qDebug() << "onLayoutsReceived: success=" << success << "error=" << error;
    if (!success) {
        qWarning() << "Failed to load layouts:" << error;
        return;
    }

    QJsonArray data = response["data"].toArray();
    m_layouts.clear();

    for (const QJsonValue& v : data) {
        m_layouts.append(PlayerLayout::fromJson(v.toObject()));
    }

    qDebug() << "onLayoutsReceived: loaded" << m_layouts.size() << "layouts";

    m_navBar->setLayouts(m_layouts);

    if (!m_layouts.isEmpty()) {
        m_currentLayout = m_layouts.first();
        qDebug() << "onLayoutsReceived: calculating layout with first layout, cols=" << m_currentLayout.cols << "rows=" << m_currentLayout.rows;
        calculateVideoLayout(m_currentLayout);
    } else {
        qDebug() << "onLayoutsReceived: no layouts loaded, video area will be empty";
    }
}

void VideoPlayerContainer::onLayoutSelected(int index) {
    if (index < 0 || index >= m_layouts.size()) return;

    m_selectedItemId.clear();
    m_currentLayout = m_layouts[index];
    calculateVideoLayout(m_currentLayout);
}

void VideoPlayerContainer::calculateVideoLayout(const PlayerLayout& layout) {
    const int margin = 4;
    const int spacing = 4;

    int availableW = m_videoArea->width();
    int availableH = m_videoArea->height();

    if (availableW <= 0 || availableH <= 0 || layout.cols <= 0 || layout.rows <= 0) {
        return;
    }

    int cw = (availableW - margin * 2 - spacing * (layout.cols - 1)) / layout.cols;
    int ch = (availableH - margin * 2 - spacing * (layout.rows - 1)) / layout.rows;

    QSet<QString> currentItemIds;
    for (const auto& item : layout.layoutJson) {
        currentItemIds.insert(item.id);

        int x = margin + item.col * (cw + spacing);
        int y = margin + item.row * (ch + spacing);
        int w = item.colSpan * cw + (item.colSpan - 1) * spacing;
        int h = item.rowSpan * ch + (item.rowSpan - 1) * spacing;

        VideoPlayer* player = VideoWidgetManager::instance().getPlayer(item.id);
        if (!player) {
            player = new VideoPlayer(m_videoArea);
            VideoWidgetManager::instance().registerPlayer(item.id, player);
            connect(player, &VideoPlayer::clicked, this, [this, itemId = item.id]() {
                onVideoPlayerClicked(itemId);
            });
        }

        player->setGeometry(x, y, w, h);
        player->setSelected(item.id == m_selectedItemId);
        player->show();
    }

    for (const QString& itemId : VideoWidgetManager::instance().getPlayerIds()) {
        if (!currentItemIds.contains(itemId)) {
            VideoWidgetManager::instance().unregisterPlayer(itemId);
        }
    }
}

void VideoPlayerContainer::onVideoPlayerClicked(const QString& itemId) {
    QString oldSelected = m_selectedItemId;
    m_selectedItemId = itemId;

    if (!oldSelected.isEmpty()) {
        VideoPlayer* oldPlayer = VideoWidgetManager::instance().getPlayer(oldSelected);
        if (oldPlayer) oldPlayer->setSelected(false);
    }

    VideoPlayer* newPlayer = VideoWidgetManager::instance().getPlayer(itemId);
    if (newPlayer) newPlayer->setSelected(true);
}

void VideoPlayerContainer::onStreamRequested(const QString& topic) {
    Q_UNUSED(topic)
}

void VideoPlayerContainer::playStream(const QString& itemId, const QString& url, const QString& title) {
    QString targetId = itemId;

    if (targetId.isEmpty()) {
        targetId = selectTargetItem();
    }

    VideoPlayer* player = VideoWidgetManager::instance().getPlayer(targetId);
    if (player) {
        m_playingUrls[targetId] = url;
        player->play(url);
    }
}

QString VideoPlayerContainer::selectTargetItem() {
    if (!m_selectedItemId.isEmpty()) {
        VideoPlayer* selected = VideoWidgetManager::instance().getPlayer(m_selectedItemId);
        if (selected && !selected->isPlaying()) {
            return m_selectedItemId;
        }
    }

    for (const auto& item : m_currentLayout.layoutJson) {
        VideoPlayer* player = VideoWidgetManager::instance().getPlayer(item.id);
        if (player && !player->isPlaying()) {
            return item.id;
        }
    }

    if (!m_currentLayout.layoutJson.isEmpty()) {
        return m_currentLayout.layoutJson.first().id;
    }

    return QString();
}

void VideoPlayerContainer::dragEnterEvent(QDragEnterEvent* event) {
    if (event->mimeData()->hasFormat("application/x-rustcam-device")) {
        event->acceptProposedAction();
    }
}

void VideoPlayerContainer::dragMoveEvent(QDragMoveEvent* event) {
    if (event->mimeData()->hasFormat("application/x-rustcam-device")) {
        event->acceptProposedAction();
    }
}

void VideoPlayerContainer::dropEvent(QDropEvent* event) {
    if (event->mimeData()->hasFormat("application/x-rustcam-device")) {
        QByteArray data = event->mimeData()->data("application/x-rustcam-device");
        QJsonDocument doc = QJsonDocument::fromJson(data);
        QVariantMap meta = doc.object().toVariantMap();
        handleDrop(meta);
        event->acceptProposedAction();
    }
}

void VideoPlayerContainer::handleDrop(const QVariantMap& meta) {
    QString type = meta["type"].toString();
    if (type == "region" || type == "group" || type == "device") {
        QString key = meta["key"].toString();
        int deviceId = key.toInt();
        if (deviceId > 0) {
            QString url = QString("rtsp://localhost/live/%1").arg(deviceId);
            QString targetId = selectTargetItem();
            playStream(targetId, url, meta["name"].toString());
        }
    }
}

void VideoPlayerContainer::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);
    if (!m_currentLayout.layoutJson.isEmpty() && m_videoArea && m_videoArea->width() > 0 && m_videoArea->height() > 0) {
        calculateVideoLayout(m_currentLayout);
    }
}

}
