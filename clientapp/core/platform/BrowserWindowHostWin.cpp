#include "platform/BrowserWindowHost.h"

#include <QRect>

#ifdef Q_OS_WIN
#include <windows.h>
#endif

namespace rustcam {

#ifdef Q_OS_WIN

namespace {

class WinBrowserWindowHost : public BrowserWindowHost {
public:
    bool supportsEmbedding() const override { return true; }

    bool attach(WId host, WId remote, const QRect& bounds) override
    {
        if (!host || !remote || !IsWindow(reinterpret_cast<HWND>(remote))) {
            return false;
        }
        const HWND hostHwnd = reinterpret_cast<HWND>(host);
        const HWND remoteHwnd = reinterpret_cast<HWND>(remote);

        // 宿主窗口加 WS_CLIPCHILDREN：父窗口绘制时跳过外来子窗口区域，
        // 避免双绘与闪烁（spec §5.6）。
        const LONG_PTR style = GetWindowLongPtrW(hostHwnd, GWL_STYLE);
        SetWindowLongPtrW(hostHwnd, GWL_STYLE, style | WS_CLIPCHILDREN);

        SetWindowPos(remoteHwnd, nullptr, bounds.x(), bounds.y(),
                     bounds.width(), bounds.height(),
                     SWP_NOZORDER | SWP_NOACTIVATE);
        return true;
    }

    void setBounds(WId remote, const QRect& bounds) override
    {
        if (!remote || !IsWindow(reinterpret_cast<HWND>(remote))) {
            return;
        }
        SetWindowPos(reinterpret_cast<HWND>(remote), nullptr,
                     bounds.x(), bounds.y(), bounds.width(), bounds.height(),
                     SWP_NOZORDER | SWP_NOACTIVATE);
    }

    void setVisible(WId remote, bool visible) override
    {
        if (!remote || !IsWindow(reinterpret_cast<HWND>(remote))) {
            return;
        }
        ShowWindow(reinterpret_cast<HWND>(remote), visible ? SW_SHOW : SW_HIDE);
    }

    void detach(WId remote) override
    {
        if (!remote || !IsWindow(reinterpret_cast<HWND>(remote))) {
            return;
        }
        const HWND remoteHwnd = reinterpret_cast<HWND>(remote);
        ShowWindow(remoteHwnd, SW_HIDE);
        // 解除父子关系，防止宿主窗口销毁时连带销毁远端窗口。
        // 这是 clientapp 唯一允许的结构性操作（spec §5.1）。
        // spike 实测：SetParent 跨进程可用，且远端窗口在脱离后仍然存活。
        SetParent(remoteHwnd, nullptr);
    }

    bool isValid(WId remote) const override
    {
        return remote != 0 && IsWindow(reinterpret_cast<HWND>(remote));
    }
};

} // namespace

BrowserWindowHost* createBrowserWindowHost()
{
    return new WinBrowserWindowHost();
}

const char* browserHostKind()
{
    return "win32";
}

#endif // Q_OS_WIN

// 非 Windows 平台不提供任何符号：工厂函数由对应平台的实现文件唯一提供，
// 避免多个实现同时编译进同一个 target 时出现重复符号。

} // namespace rustcam
