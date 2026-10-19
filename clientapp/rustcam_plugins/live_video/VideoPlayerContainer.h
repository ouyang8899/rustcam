#pragma once

#include <QWidget>
#include <QVBoxLayout>
#include <QList>
#include <QDropEvent>
#include <QDragEnterEvent>
#include <QMimeData>
#include <QJsonDocument>
#include <QJsonObject>
#include "core/PlayerLayout.h"
#include "core/VideoWidgetManager.h"
#include "core/ServerApi.h"
#include "core/DataManager.h"
#include "core/VideoPlayerToolbar.h"

namespace rustcam {

class NavigationBar;

class VideoPlayerContainer : public QWidget {
    Q_OBJECT
public:
    explicit VideoPlayerContainer(QWidget* parent = nullptr);
    ~VideoPlayerContainer();

    void playStream(const QString& itemId, const QString& url, const QString& title);
    QString getSelectedItemId() const { return m_selectedItemId; }

public slots:
    void initialize();
    void onStreamRequested(const QString& topic);

private slots:
    void onLayoutsReady();
    void onLayoutSelected(int index);
    void onVideoPlayerClicked(const QString& itemId);
    void onChannelPlayLinksReceived(bool success, const QJsonObject& response, const QString& error, const QString& title, const QString& targetId);

private:
    void calculateVideoLayout(const PlayerLayout& layout);
    QString selectTargetItem();
    void handleDrop(const QVariantMap& meta);

protected:
    void dragEnterEvent(QDragEnterEvent* event) override;
    void dragMoveEvent(QDragMoveEvent* event) override;
    void dropEvent(QDropEvent* event) override;
    void resizeEvent(QResizeEvent* event) override;

    QWidget* m_videoArea;
    NavigationBar* m_navBar;
    QList<PlayerLayout> m_layouts;
    PlayerLayout m_currentLayout;
    QString m_selectedItemId;
    QMap<QString, QString> m_playingUrls;
    QMap<QString, QWidget*> m_playerWrappers;
};

}
