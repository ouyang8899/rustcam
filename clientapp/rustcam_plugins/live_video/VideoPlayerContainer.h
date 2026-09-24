#pragma once

#include <QWidget>
#include <QVBoxLayout>
#include <QList>
#include "core/PlayerLayout.h"
#include "core/VideoWidgetManager.h"
#include "core/ServerApi.h"

namespace rustcam {

class NavigationBar;

class VideoPlayerContainer : public QWidget {
    Q_OBJECT
public:
    explicit VideoPlayerContainer(QWidget* parent = nullptr);
    ~VideoPlayerContainer();

private slots:
    void onLayoutsReceived(bool success, const QJsonObject& response, const QString& error);
    void onLayoutSelected(int index);

private:
    void loadLayouts();
    void calculateVideoLayout(const PlayerLayout& layout);

    QWidget* m_videoArea;
    NavigationBar* m_navBar;
    QList<PlayerLayout> m_layouts;
    PlayerLayout m_currentLayout;
};

}
