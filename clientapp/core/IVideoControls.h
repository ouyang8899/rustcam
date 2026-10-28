#pragma once

#include <QString>

class QWidget;

namespace rustcam {

/// 视频画面控件对外暴露的可控能力，供悬浮工具栏统一调用。
/// 实时播放器与录像回放播放器都实现该接口。
class IVideoControls {
public:
    virtual ~IVideoControls() = default;

    virtual QWidget* controlWidget() = 0;

    virtual void stop() = 0;
    /// 工具栏关闭按钮：默认仅停止播放，回放播放器可重写为关闭格子。
    virtual void requestClose() { stop(); }
    virtual bool isPlaying() const = 0;

    virtual void setRotation(int angle) = 0;
    virtual void setMirror(bool horizontal, bool vertical) = 0;
    virtual bool takeScreenshot(const QString& filePath) = 0;
    virtual void setZoomEnabled(bool enabled) = 0;
    virtual void setAudioEnabled(bool enabled) = 0;
    virtual void setMediaRateVisible(bool) {}
    virtual void toggleFullscreen() = 0;
    virtual void setSelected(bool selected) = 0;
    virtual void setWatermarkAccount(const QString&) {}
    virtual void setDeviceName(const QString&) {}
    /// 通道/设备名称，用于截图文件名等场景
    virtual QString deviceName() const { return QString(); }
};

} // namespace rustcam
