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
/// 从设备树拖动通道时用的 mime 类型
const char* const kDeviceMimeType = "application/x-rustcam-device";
/// 选中边框宽度（播放器内缩，避免画面盖住边框）
constexpr int kBorderWidth = 2;
}

PlaybackCell::PlaybackCell(QWidget* parent)
    : QWidget(parent)
    , m_selected(false)
{
    setObjectName("PlaybackCell");
    setAutoFillBackground(true);
    setProperty("selected", false);
    // 允许把别的播放器拖到本格子上
    setAcceptDrops(true);

    // 格子拥有播放器：连接在这里一次建立，之后不再变动
    m_playerWidget = new PlaybackPlayerWidget(this);
    connect(m_playerWidget, &PlaybackPlayerWidget::clicked, this, &PlaybackCell::onPlayerClicked);
    connect(m_playerWidget, &PlaybackPlayerWidget::playbackProgressChanged,
            this, &PlaybackCell::onPlayerProgress);
    connect(m_playerWidget, &PlaybackPlayerWidget::closeRequested,
            this, [this]() { emit closeRequested(m_layoutItemId); });
}

PlaybackCell::~PlaybackCell() {
}

void PlaybackCell::onPlayerClicked(const QString& deviceId) {
    emit clicked(m_layoutItemId, deviceId);
}

void PlaybackCell::onPlayerProgress(int64_t absoluteTimeMs) {
    emit playbackProgressChanged(m_layoutItemId, absoluteTimeMs);
}

void PlaybackCell::setSelected(bool selected) {
    if (m_selected != selected) {
        m_selected = selected;
        setProperty("selected", selected);
        // 属性选择器需要重新 polish 才会生效
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
        || event->mimeData()->hasFormat(kRecordingMimeType)
        || event->mimeData()->hasFormat(kDeviceMimeType)) {
        event->acceptProposedAction();
    }
}

void PlaybackCell::dragMoveEvent(QDragMoveEvent* event) {
    if (event->mimeData()->hasFormat(kPlayerMimeType)
        || event->mimeData()->hasFormat(kRecordingMimeType)
        || event->mimeData()->hasFormat(kDeviceMimeType)) {
        event->acceptProposedAction();
    }
}

void PlaybackCell::dropEvent(QDropEvent* event) {
    if (event->mimeData()->hasFormat(kPlayerMimeType)) {
        const QString sourceUid = QString::fromUtf8(event->mimeData()->data(kPlayerMimeType));
        if (sourceUid.isEmpty()) {
            return;
        }
        emit playerDropped(m_layoutItemId, sourceUid);
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
        emit recordingDropped(m_layoutItemId, channelKey, static_cast<int64_t>(obj["start_time_ms"].toDouble()));
        event->acceptProposedAction();
        return;
    }

    if (event->mimeData()->hasFormat(kDeviceMimeType)) {
        // 设备树通道拖到窗口：把该通道绑定到这个窗口
        const QJsonObject obj = QJsonDocument::fromJson(
            event->mimeData()->data(kDeviceMimeType)).object();
        if (obj["type"].toString() != "channel") {
            return;
        }
        const QString deviceTag = obj["device_tag"].toString();
        const QString channelTag = obj["channel_tag"].toString();
        if (deviceTag.isEmpty() || channelTag.isEmpty()) {
            return;
        }
        emit channelDropped(m_layoutItemId, deviceTag, channelTag, obj["name"].toString());
        event->acceptProposedAction();
    }
}

void PlaybackCell::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);
    if (m_playerWidget) {
        m_playerWidget->setGeometry(kBorderWidth, kBorderWidth,
                                    width() - kBorderWidth * 2,
                                    height() - kBorderWidth * 2);
    }
}

void PlaybackCell::paintEvent(QPaintEvent* event) {
    QStyleOption opt;
    opt.init(this);
    QPainter p(this);
    style()->drawPrimitive(QStyle::PE_Widget, &opt, &p, this);
}

}
