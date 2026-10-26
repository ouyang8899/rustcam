#include "PlaybackCell.h"
#include "PlaybackPlayerWidget.h"
#include <QVBoxLayout>
#include <QPainter>
#include <QStyle>
#include <QStyleOption>
#include <QDragEnterEvent>
#include <QDragMoveEvent>
#include <QDropEvent>
#include <QMimeData>
#include <QJsonDocument>
#include <QJsonObject>

namespace rustcam {

namespace {
/// 拖动播放器时用的 mime 类型
const char* const kPlayerMimeType = "application/x-rustcam-player";
/// 拖动录像文件时用的 mime 类型
const char* const kRecordingMimeType = "application/x-rustcam-recording";
}

PlaybackCell::PlaybackCell(QWidget* parent)
    : QWidget(parent)
    , m_selected(false)
{
    setObjectName("PlaybackCell");
    setAutoFillBackground(true);
    // 允许把别的播放器拖到本格子上
    setAcceptDrops(true);
}

PlaybackCell::~PlaybackCell() {
}

PlaybackPlayerWidget* PlaybackCell::takePlayerWidget() {
    PlaybackPlayerWidget* widget = m_playerWidget;
    if (widget) {
        // 断开与本格子的连线，避免播放器换格子后重复上报
        disconnect(widget, nullptr, this, nullptr);
        widget->setParent(nullptr);
        m_playerWidget = nullptr;
    }
    return widget;
}

void PlaybackCell::setPlayerWidget(PlaybackPlayerWidget* widget) {
    if (m_playerWidget == widget) {
        return;
    }
    if (m_playerWidget) {
        m_playerWidget->setParent(nullptr);
    }
    m_playerWidget = widget;
    if (!widget) {
        return;
    }
    widget->setParent(this);
    widget->setGeometry(0, 0, width(), height());
    widget->show();
    connect(widget, &PlaybackPlayerWidget::clicked, this, &PlaybackCell::clicked);
    connect(widget, &PlaybackPlayerWidget::playbackProgressChanged,
            this, &PlaybackCell::playbackProgressChanged);
}

void PlaybackCell::setSelected(bool selected) {
    if (m_selected != selected) {
        m_selected = selected;
        setProperty("selected", selected);
        style()->unpolish(this);
        style()->polish(this);
        update();
        if (m_playerWidget) {
            m_playerWidget->setSelected(selected);
        }
    }
}

void PlaybackCell::dragEnterEvent(QDragEnterEvent* event) {
    if (event->mimeData()->hasFormat(kPlayerMimeType)
        || event->mimeData()->hasFormat(kRecordingMimeType)) {
        event->acceptProposedAction();
    }
}

void PlaybackCell::dragMoveEvent(QDragMoveEvent* event) {
    if (event->mimeData()->hasFormat(kPlayerMimeType)
        || event->mimeData()->hasFormat(kRecordingMimeType)) {
        event->acceptProposedAction();
    }
}

void PlaybackCell::dropEvent(QDropEvent* event) {
    if (event->mimeData()->hasFormat(kPlayerMimeType)) {
        const QString sourceUid = QString::fromUtf8(event->mimeData()->data(kPlayerMimeType));
        if (sourceUid.isEmpty()) {
            return;
        }
        emit playerDropped(sourceUid);
        event->acceptProposedAction();
        return;
    }

    if (event->mimeData()->hasFormat(kRecordingMimeType)) {
        // 录像文件拖到窗口：那个窗口播这个通道
        const QJsonObject obj = QJsonDocument::fromJson(
            event->mimeData()->data(kRecordingMimeType)).object();
        const QString channelKey = obj["channel_key"].toString();
        if (channelKey.isEmpty()) {
            return;
        }
        emit recordingDropped(channelKey, static_cast<int64_t>(obj["start_time_ms"].toDouble()));
        event->acceptProposedAction();
    }
}

void PlaybackCell::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);
    if (m_playerWidget) {
        m_playerWidget->setGeometry(0, 0, width(), height());
    }
}

void PlaybackCell::paintEvent(QPaintEvent* event) {
    QStyleOption opt;
    opt.init(this);
    QPainter p(this);
    style()->drawPrimitive(QStyle::PE_Widget, &opt, &p, this);
}

}
