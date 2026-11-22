#pragma once

#include <QByteArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonParseError>
#include <QString>

// clientapp 与 rustcam-browser 之间的进程间契约。
//
// 传输：Qt QLocalSocket（Windows 为命名管道，Linux/macOS 为 Unix domain socket）。
// 帧：4 字节大端无符号长度（不含长度字段本身）+ UTF-8 JSON。
//
// 本头文件刻意不依赖 CEF，也不依赖 Qt Widgets —— rustcam-browser 进程只允许
// 包含本文件（spec 规则 R2）。
namespace rustcam {
namespace browseripc {

/// 协议版本。握手时双方比对，不一致则断开。
inline constexpr int kProtocolVersion = 1;

/// 单帧最大字节数（不含 4 字节长度前缀）。
inline constexpr int kMaxFrameSize = 4 * 1024 * 1024;

namespace msg {
inline constexpr const char* kHello = "hello";
inline constexpr const char* kHelloOk = "hello.ok";
inline constexpr const char* kSessionCreate = "session.create";
inline constexpr const char* kSessionCreated = "session.created";
inline constexpr const char* kSessionClose = "session.close";
inline constexpr const char* kSessionClosed = "session.closed";
inline constexpr const char* kSessionExec = "session.exec";
inline constexpr const char* kSessionReady = "session.ready";
inline constexpr const char* kSessionEvent = "session.event";
inline constexpr const char* kSessionFailed = "session.failed";
inline constexpr const char* kSessionSetBounds = "session.setBounds";
inline constexpr const char* kSessionSetVisible = "session.setVisible";
inline constexpr const char* kSessionFocus = "session.focus";
inline constexpr const char* kShutdown = "shutdown";
inline constexpr const char* kError = "error";
} // namespace msg

namespace err {
inline constexpr const char* kBadJson = "bad_json";
inline constexpr const char* kBadRequest = "bad_request";
inline constexpr const char* kUnknownType = "unknown_type";
inline constexpr const char* kNoSuchSession = "no_such_session";
inline constexpr const char* kModeUnsupported = "mode_unsupported";
} // namespace err

namespace hostkind {
inline constexpr const char* kWin32 = "win32";
inline constexpr const char* kX11 = "x11";
inline constexpr const char* kNone = "none";
} // namespace hostkind

namespace mode {
inline constexpr const char* kEmbedded = "embedded";
inline constexpr const char* kOverlay = "overlay";
} // namespace mode

/// 编码一帧：4 字节大端长度 + 紧凑 JSON。
inline QByteArray encodeFrame(const QJsonObject& message)
{
    const QByteArray payload = QJsonDocument(message).toJson(QJsonDocument::Compact);
    QByteArray frame;
    frame.reserve(4 + payload.size());
    const quint32 length = static_cast<quint32>(payload.size());
    frame.append(static_cast<char>((length >> 24) & 0xFF));
    frame.append(static_cast<char>((length >> 16) & 0xFF));
    frame.append(static_cast<char>((length >> 8) & 0xFF));
    frame.append(static_cast<char>(length & 0xFF));
    frame.append(payload);
    return frame;
}

/// 解码一帧。frame 必须是完整的一帧（含长度前缀）。
inline bool decodeFrame(const QByteArray& frame, QJsonObject* out)
{
    if (frame.size() < 4) {
        return false;
    }
    const quint32 length = (static_cast<quint8>(frame[0]) << 24)
                         | (static_cast<quint8>(frame[1]) << 16)
                         | (static_cast<quint8>(frame[2]) << 8)
                         | static_cast<quint8>(frame[3]);
    if (length == 0 || length > static_cast<quint32>(kMaxFrameSize)) {
        return false;
    }
    if (frame.size() != static_cast<int>(length) + 4) {
        return false;
    }

    QJsonParseError err {};
    const QJsonDocument doc = QJsonDocument::fromJson(frame.mid(4), &err);
    if (err.error != QJsonParseError::NoError || !doc.isObject()) {
        return false;
    }
    if (out) {
        *out = doc.object();
    }
    return true;
}

/// 原生句柄的十进制字符串表示（HWND / XID）。
/// 句柄可能是 64 位，JSON number 会丢精度，因此一律用字符串。
inline QString encodeHandle(quint64 handle)
{
    return QString::number(handle);
}

/// 解析句柄字符串。
inline bool decodeHandle(const QString& text, quint64* out)
{
    if (text.isEmpty()) {
        return false;
    }
    bool ok = false;
    const qulonglong value = text.toULongLong(&ok);
    if (!ok) {
        return false;
    }
    if (out) {
        *out = static_cast<quint64>(value);
    }
    return true;
}

} // namespace browseripc
} // namespace rustcam
