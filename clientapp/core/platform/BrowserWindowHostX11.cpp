#include "platform/BrowserWindowHost.h"

#include <QRect>

#ifdef Q_OS_LINUX
#include <QGuiApplication>
#include <X11/Xlib.h>
#endif

namespace rustcam {

#ifdef Q_OS_LINUX

namespace {

class X11BrowserWindowHost : public BrowserWindowHost {
public:
    bool supportsEmbedding() const override
    {
        // 判据不是「是不是 Wayland」，而是「自己有没有 XID 可当 parent」。
        // 原生 Wayland QPA 下 clientapp 不是 X11 客户端 → 只能 overlay。
        if (qgetenv("DISPLAY").isEmpty()) {
            return false;
        }
        if (qApp == nullptr) {
            return false;
        }
        return qApp->platformName().startsWith("xcb");
    }

    bool attach(WId host, WId remote, const QRect& bounds) override
    {
        if (!host || !remote || !isValid(remote)) {
            return false;
        }
        Display* display = openDisplay();
        if (!display) {
            return false;
        }
        // X11 允许跨 client reparent（XEmbed 即此机制）。
        XReparentWindow(display, remote, host, bounds.x(), bounds.y());
        XMoveResizeWindow(display, remote, bounds.x(), bounds.y(),
                          static_cast<unsigned int>(bounds.width()),
                          static_cast<unsigned int>(bounds.height()));
        XFlush(display);
        return true;
    }

    void setBounds(WId remote, const QRect& bounds) override
    {
        if (!remote || !isValid(remote)) {
            return;
        }
        Display* display = openDisplay();
        if (!display) {
            return;
        }
        XMoveResizeWindow(display, remote, bounds.x(), bounds.y(),
                          static_cast<unsigned int>(bounds.width()),
                          static_cast<unsigned int>(bounds.height()));
        XFlush(display);
    }

    void setVisible(WId remote, bool visible) override
    {
        if (!remote || !isValid(remote)) {
            return;
        }
        Display* display = openDisplay();
        if (!display) {
            return;
        }
        if (visible) {
            XMapWindow(display, remote);
        } else {
            XUnmapWindow(display, remote);
        }
        XFlush(display);
    }

    void detach(WId remote) override
    {
        if (!remote || !isValid(remote)) {
            return;
        }
        Display* display = openDisplay();
        if (!display) {
            return;
        }
        XUnmapWindow(display, remote);
        XReparentWindow(display, remote, DefaultRootWindow(display), 0, 0);
        XFlush(display);
    }

    bool isValid(WId remote) const override
    {
        if (!remote) {
            return false;
        }
        Display* display = openDisplay();
        if (!display) {
            return false;
        }
        XWindowAttributes attrs;
        const Status ok = XGetWindowAttributes(display, remote, &attrs);
        return ok != 0;
    }

private:
    static Display* openDisplay()
    {
        static Display* display = nullptr;
        if (!display) {
            display = XOpenDisplay(nullptr);
        }
        return display;
    }
};

} // namespace

BrowserWindowHost* createBrowserWindowHost()
{
    return new X11BrowserWindowHost();
}

const char* browserHostKind()
{
    return "x11";
}

#endif // Q_OS_LINUX

// 非 Linux 平台不提供任何符号：工厂函数由对应平台的实现文件唯一提供，
// 避免多个实现同时编译进同一个 target 时出现重复符号。

} // namespace rustcam
