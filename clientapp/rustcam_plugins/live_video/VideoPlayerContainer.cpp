#include "VideoPlayerContainer.h"
#include "NavigationBar.h"
#include "core/VideoPlayer.h"
#include <QVBoxLayout>
#include <QDebug>

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
    m_videoArea->setStyleSheet("background-color: #1a1a1a;");

    m_navBar = new NavigationBar(this);
    m_navBar->setStyleSheet("background-color: #2a2a2a;");

    layout->addWidget(m_videoArea, 1);
    layout->addWidget(m_navBar, 0);

    connect(m_navBar, &NavigationBar::layoutSelected, this, &VideoPlayerContainer::onLayoutSelected);

    loadLayouts();
}

VideoPlayerContainer::~VideoPlayerContainer() {
    VideoWidgetManager::instance().clearLayout();
}

void VideoPlayerContainer::loadLayouts() {
    ServerApi::instance()->get("/api/v1/layouts", [this](bool success, const QJsonObject& response, const QString& error) {
        QMetaObject::invokeMethod(this, [this, success, response, error]() {
            onLayoutsReceived(success, response, error);
        }, Qt::QueuedConnection);
    });
}

void VideoPlayerContainer::onLayoutsReceived(bool success, const QJsonObject& response, const QString& error) {
    if (!success) {
        qWarning() << "Failed to load layouts:" << error;
        return;
    }

    QJsonArray data = response["data"].toArray();
    m_layouts.clear();

    for (const QJsonValue& v : data) {
        m_layouts.append(PlayerLayout::fromJson(v.toObject()));
    }

    m_navBar->setLayouts(m_layouts);

    if (!m_layouts.isEmpty()) {
        m_currentLayout = m_layouts.first();
        calculateVideoLayout(m_currentLayout);
    }
}

void VideoPlayerContainer::onLayoutSelected(int index) {
    if (index < 0 || index >= m_layouts.size()) return;

    m_currentLayout = m_layouts[index];
    calculateVideoLayout(m_currentLayout);
}

void VideoPlayerContainer::calculateVideoLayout(const PlayerLayout& layout) {
    VideoWidgetManager::instance().clearLayout();

    const int margin = 2;
    const int spacing = 2;

    int availableW = m_videoArea->width();
    int availableH = m_videoArea->height();

    if (availableW <= 0 || availableH <= 0 || layout.cols <= 0 || layout.rows <= 0) {
        return;
    }

    int cw = (availableW - margin * 2 - spacing * (layout.cols - 1)) / layout.cols;
    int ch = (availableH - margin * 2 - spacing * (layout.rows - 1)) / layout.rows;

    for (const auto& item : layout.layoutJson) {
        int x = margin + item.col * (cw + spacing);
        int y = margin + item.row * (ch + spacing);
        int w = item.colSpan * cw + (item.colSpan - 1) * spacing;
        int h = item.rowSpan * ch + (item.rowSpan - 1) * spacing;

        VideoPlayer* player = VideoWidgetManager::instance().getPlayer(item.id);
        if (!player) {
            player = new VideoPlayer(m_videoArea);
            VideoWidgetManager::instance().registerPlayer(item.id, player);
        }

        player->setGeometry(x, y, w, h);
        player->show();
    }
}

}
