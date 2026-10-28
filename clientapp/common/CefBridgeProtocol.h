#pragma once

#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonParseError>
#include <QString>

// Wire contract between the host executable (render process side) and the map
// plugin (browser process side).
//
// The render process injects kNativeFunctionName into the page's global scope.
// Calling it with a JSON string sends that string to the browser process as a
// CefProcessMessage named kNativeMessageName. The plugin parses it with
// parseBridgeMessage() and dispatches on the "type" field.
namespace rustcam {
namespace cefbridge {

inline constexpr const char* kNativeMessageName = "rustcam.native";
inline constexpr const char* kNativeFunctionName = "rustcamNative";

namespace event {
inline constexpr const char* kMapReady = "mapReady";
inline constexpr const char* kMarkerClicked = "markerClicked";
inline constexpr const char* kViewChanged = "mapViewChanged";
} // namespace event

// Parses a bridge payload. Returns false (leaving |out| untouched) when the
// payload is not a JSON object or has no "type" field.
inline bool parseBridgeMessage(const QString& payload, QJsonObject* out)
{
    QJsonParseError err {};
    const QJsonDocument doc = QJsonDocument::fromJson(payload.toUtf8(), &err);
    if (err.error != QJsonParseError::NoError || !doc.isObject()) {
        return false;
    }
    const QJsonObject obj = doc.object();
    if (!obj.contains(QStringLiteral("type"))) {
        return false;
    }
    if (out) {
        *out = obj;
    }
    return true;
}

} // namespace cefbridge
} // namespace rustcam
