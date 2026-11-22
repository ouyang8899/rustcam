#pragma once

#include <QtGlobal>

// cef_base.h 按平台分发 include/internal/cef_win.h / cef_mac.h / cef_linux.h，
// CefWindowHandle 定义在其中（CEF 156 没有独立的 cef_types.h）。
#include "include/cef_base.h"

// overlay 模式（browser 进程自开顶层窗口）下的原生窗口操作。
// embedded 模式不需要这些操作——几何由 clientapp 直接跨进程操作远端窗口。
namespace rustcam {
namespace browser {

/// 把 CEF 的原生窗口句柄转成可跨进程传递的 64 位整数。
/// Windows 上是 HWND（指针），X11 上是 XID（整数），两种都要支持。
quint64 toPortableHandle(CefWindowHandle hwnd);

/// fromPortableHandle 的逆操作：把 64 位整数还原成 CEF 的原生窗口句柄。
CefWindowHandle fromPortableHandle(quint64 handle);

/// 移动并缩放顶层窗口。
void overlayMoveResize(CefWindowHandle hwnd, int x, int y, int w, int h);

/// 显示 / 隐藏顶层窗口。
void overlaySetVisible(CefWindowHandle hwnd, bool visible);

/// 请求把键盘焦点给顶层窗口。
void overlayFocus(CefWindowHandle hwnd);

} // namespace browser
} // namespace rustcam
