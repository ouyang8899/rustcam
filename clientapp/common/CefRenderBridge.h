#pragma once

#include "include/cef_process_message.h"
#include "include/cef_render_process_handler.h"
#include "include/cef_v8.h"

#include "common/CefBridgeProtocol.h"

// Render-process side of the C++ <-> JS bridge.
//
// The renderer process is entered through CefExecuteProcess() before any
// plugin is loaded, so this half of the bridge lives in the host executable.
// The browser-process half lives in the map plugin.
//
// This header is intentionally header-only so that it can be reused by the
// host executable and by tests without introducing a link dependency.
namespace rustcam {
namespace cefbridge {

// Native function exposed to the page as window.rustcamNative(json).
class NativeBridgeHandler : public CefV8Handler {
public:
    explicit NativeBridgeHandler(CefRefPtr<CefFrame> frame)
        : m_frame(frame)
    {
    }

    bool Execute(const CefString& /*name*/,
                 CefRefPtr<CefV8Value> /*object*/,
                 const CefV8ValueList& arguments,
                 CefRefPtr<CefV8Value>& retval,
                 CefString& exception) override
    {
        if (!m_frame || arguments.empty() || !arguments[0]->IsString()) {
            exception = "rustcamNative() expects a JSON string argument";
            retval = CefV8Value::CreateUndefined();
            return true;
        }

        CefRefPtr<CefProcessMessage> message = CefProcessMessage::Create(kNativeMessageName);
        message->GetArgumentList()->SetString(0, arguments[0]->GetStringValue());
        m_frame->SendProcessMessage(PID_BROWSER, message);
        retval = CefV8Value::CreateUndefined();
        return true;
    }

private:
    IMPLEMENT_REFCOUNTING(NativeBridgeHandler);

    CefRefPtr<CefFrame> m_frame;
};

// Injects window.rustcamNative() into every V8 context of the renderer.
class RenderProcessBridge : public CefRenderProcessHandler {
public:
    void OnContextCreated(CefRefPtr<CefBrowser> /*browser*/,
                           CefRefPtr<CefFrame> frame,
                           CefRefPtr<CefV8Context> context) override
    {
        CefRefPtr<CefV8Handler> handler = new NativeBridgeHandler(frame);
        CefRefPtr<CefV8Value> function = CefV8Value::CreateFunction(kNativeFunctionName,
                                                                     handler);
        context->GetGlobal()->SetValue(kNativeFunctionName,
                                       function,
                                       CefV8Value::PropertyAttribute::V8_PROPERTY_ATTRIBUTE_NONE);
    }
};

} // namespace cefbridge
} // namespace rustcam
