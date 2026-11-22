#include "platform/BrowserWindowHost.h"

#include <QRect>

// 非 Windows / 非 Linux 的平台：没有可用的跨进程嵌入机制，
// 一律降级为 overlay 模式（browser 进程自开顶层窗口，clientapp 下发屏幕矩形）。
//
// 同时作为「X11 不可用时的兜底实现」被 Linux 构建选用。
//
// 本文件只在 CMake 选中它时（定义 RUSTCAM_BROWSER_HOST_FALLBACK 宏）才提供符号，
// 保证任何平台上工厂函数只有唯一一份实现。

#ifdef RUSTCAM_BROWSER_HOST_FALLBACK

namespace rustcam {

namespace {

class FallbackBrowserWindowHost : public BrowserWindowHost {
public:
    bool supportsEmbedding() const override { return false; }

    bool attach(WId, WId, const QRect&) override { return false; }

    void setBounds(WId, const QRect&) override {}

    void setVisible(WId, bool) override {}

    void detach(WId) override {}

    bool isValid(WId) const override { return false; }
};

} // namespace

BrowserWindowHost* createBrowserWindowHost()
{
    return new FallbackBrowserWindowHost();
}

const char* browserHostKind()
{
    return "none";
}

} // namespace rustcam

#endif // RUSTCAM_BROWSER_HOST_FALLBACK
