#pragma once

#include "SessionManager.h"

#include <QByteArray>
#include <QJsonObject>
#include <QObject>
#include <QString>

class QLocalServer;
class QLocalSocket;

namespace rustcam {
namespace browser {

/// 进程间 IPC 服务端。
///
/// 监听 QLocalServer（Windows 为命名管道），按 BrowserIpcProtocol 的帧格式
/// 收发 JSON 消息，把命令转给 SessionManager，把事件转给 clientapp。
class IpcServer : public QObject {
    Q_OBJECT

public:
    IpcServer(SessionManager* manager, QObject* parent = nullptr);

    /// 开始监听。pipeName 为逻辑名（平台前缀由 Qt 处理）。
    bool listen(const QString& pipeName, QString* error);

signals:
    void shutdownRequested();

private slots:
    void onNewConnection();
    void onReadyRead();
    void onDisconnected();
    void onSessionCreated(const QString& sessionId, quint64 handle);
    void onSessionReady(const QString& sessionId);
    void onSessionEvent(const QString& sessionId, const QString& payload);
    void onSessionClosed(const QString& sessionId);
    void onSessionFailed(const QString& sessionId, const QString& error);

private:
    void sendMessage(QLocalSocket* socket, const QJsonObject& message);
    void handleMessage(QLocalSocket* socket, const QJsonObject& message);
    void handleCreateSession(QLocalSocket* socket, const QJsonObject& message);
    void reportError(QLocalSocket* socket, const char* code);

    SessionManager* m_manager = nullptr;
    QLocalServer* m_server = nullptr;
    QLocalSocket* m_client = nullptr;
    QByteArray m_rxBuffer;
};

} // namespace browser
} // namespace rustcam
