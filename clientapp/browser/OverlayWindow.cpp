#include "OverlayWindow.h"

#ifdef Q_OS_WIN
#include <windows.h>
#elif defined(Q_OS_LINUX)
#include <X11/Xlib.h>
#endif

namespace rustcam {
namespace browser {

quint64 toPortableHandle(CefWindowHandle hwnd)
{
#ifdef Q_OS_WIN
    // HWND 是指针。
    return static_cast<quint64>(reinterpret_cast<quintptr>(hwnd));
#else
    // X11 Window 是整数。
    return static_cast<quint64>(hwnd);
#endif
}

CefWindowHandle fromPortableHandle(quint64 handle)
{
#ifdef Q_OS_WIN
    // HWND 是指针，不能用 static_cast。
    return reinterpret_cast<CefWindowHandle>(static_cast<quintptr>(handle));
#else
    // X11 Window 是整数。
    return static_cast<CefWindowHandle>(handle);
#endif
}

#ifdef Q_OS_WIN

void overlayMoveResize(CefWindowHandle hwnd, int x, int y, int w, int h)
{
    if (!hwnd) {
        return;
    }
    SetWindowPos(hwnd, nullptr, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
}

void overlaySetVisible(CefWindowHandle hwnd, bool visible)
{
    if (!hwnd) {
        return;
    }
    ShowWindow(hwnd, visible ? SW_SHOW : SW_HIDE);
}

void overlayFocus(CefWindowHandle hwnd)
{
    if (!hwnd) {
        return;
    }
    const DWORD targetTid = GetWindowThreadProcessId(hwnd, nullptr);
    const DWORD selfTid = GetCurrentThreadId();
    AttachThreadInput(selfTid, targetTid, TRUE);
    SetFocus(hwnd);
    AttachThreadInput(selfTid, targetTid, FALSE);
}

#elif defined(Q_OS_LINUX)

namespace {

Display* overlayDisplay()
{
    static Display* display = nullptr;
    if (!display) {
        display = XOpenDisplay(nullptr);
    }
    return display;
}

} // namespace

void overlayMoveResize(CefWindowHandle hwnd, int x, int y, int w, int h)
{
    if (!hwnd) {
        return;
    }
    Display* display = overlayDisplay();
    if (!display) {
        return;
    }
    XMoveResizeWindow(display, hwnd, x, y,
                      static_cast<unsigned int>(w), static_cast<unsigned int>(h));
    XFlush(display);
}

void overlaySetVisible(CefWindowHandle hwnd, bool visible)
{
    if (!hwnd) {
        return;
    }
    Display* display = overlayDisplay();
    if (!display) {
        return;
    }
    if (visible) {
        XMapWindow(display, hwnd);
    } else {
        XUnmapWindow(display, hwnd);
    }
    XFlush(display);
}

void overlayFocus(CefWindowHandle hwnd)
{
    if (!hwnd) {
        return;
    }
    Display* display = overlayDisplay();
    if (!display) {
        return;
    }
    XRaiseWindow(display, hwnd);
    XSetInputFocus(display, hwnd, RevertToParent, CurrentTime);
    XFlush(display);
}

#else

void overlayMoveResize(CefWindowHandle, int, int, int, int) {}
void overlaySetVisible(CefWindowHandle, bool) {}
void overlayFocus(CefWindowHandle) {}

#endif

} // namespace browser
} // namespace rustcam
