#pragma once

#include "include/cef_process_message.h"
#include "include/cef_render_process_handler.h"
#include "include/cef_v8.h"

#include "common/CefBridgeProtocol.h"

// 渲染进程侧的 C++ <-> JS 桥。
//
// 渲染进程是 rustcam-browser.exe 自己 spawn 的（--type=renderer），
// 因此这一半必须住在 rustcam-browser 进程内，不能留在 clientapp。
// 载荷格式与 clientapp/common/CefBridgeProtocol.h 完全一致。
namespace rustcam {
namespace cefbridge {

/// 暴露给页面的原生函数 window.rustcamNative(json)。
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

/// 向每个 V8 context 注入 window.rustcamNative()。
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
