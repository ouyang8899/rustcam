#pragma once

#include <QRect>
#include <QString>

// Qt 的 WId 在 Windows 上是 HWND，在 X11 上是 X11 Window (XID)。
// WId 定义在 QtGui 的 qwindowdefs.h 里（不是 QtGlobal）。
#include <QtGui/qwindowdefs.h>

#include "CoreExport.h"

namespace rustcam {

/// 浏览器原生窗口的宿主策略（平台相关）。
///
/// 浏览器窗口由 rustcam-browser 进程创建，clientapp 进程只做「操作者」：
/// 调整几何、显示隐藏、请求焦点。任何会破坏 CEF 内部状态的操作
/// （销毁窗口、建立父子关系、改样式）都不允许在这里出现。
///
/// 唯一允许的结构性操作是 detach()：解除父子关系，避免宿主窗口销毁时
/// 连带销毁远端窗口（见 spec §5.4）。
class BrowserWindowHost {
public:
    virtual ~BrowserWindowHost() = default;

    /// 当前平台/会话能否把外部进程创建的窗口嵌入本进程窗口。
    virtual bool supportsEmbedding() const = 0;

    /// 把远端句柄挂到宿主窗口下并定位到 bounds。
    virtual bool attach(WId host, WId remote, const QRect& bounds) = 0;

    /// 调整远端窗口的位置与尺寸（相对其父窗口客户区）。
    virtual void setBounds(WId remote, const QRect& bounds) = 0;

    /// 显示 / 隐藏远端窗口。
    virtual void setVisible(WId remote, bool visible) = 0;

    /// 解除父子关系。必须保证远端窗口仍然存活。
    virtual void detach(WId remote) = 0;

    /// 远端句柄是否仍然有效。
    virtual bool isValid(WId remote) const = 0;
};

/// 创建当前平台/会话的实现。
RUSTCAM_CORE_API BrowserWindowHost* createBrowserWindowHost();

/// 当前实现的宿主句柄种类： "win32" / "x11" / "none"。
/// 用于填充 session.create 的 host.kind 字段。
RUSTCAM_CORE_API const char* browserHostKind();

} // namespace rustcam
