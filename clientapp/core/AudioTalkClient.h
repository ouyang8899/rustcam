#pragma once

#include <QObject>
#include <QString>
#include "CoreExport.h"

class QThread;
class QWebSocket;
class QIODevice;
class QTimer;

namespace rustcam {

/**
 * 语音广播工作线程：取票、建立 WebSocket、采集麦克风、发送 PCM 全部在这里完成。
 *
 * 取票是阻塞式 HTTP 请求；麦克风采集与网络发送也放在工作线程，避免卡住 GUI。
 */
class AudioTalkWorker : public QObject {
    Q_OBJECT
public:
    explicit AudioTalkWorker(QObject* parent = nullptr);
    ~AudioTalkWorker() override;

public slots:
    void start(const QString& deviceTag, const QString& channelTag);
    void stop();

signals:
    /// 0=Idle 1=Connecting 2=Inviting 3=Ready 4=Error
    void stateChanged(int state);
    void errorOccurred(const QString& message);

private slots:
    void onConnected();
    void onDisconnected();
    void onTextMessage(const QString& message);
    void onSocketError();
    void onAudioReadyRead();
    void onFakePcmTick();

private:
    void requestTicket();
    void connectWithTicket(const QString& ticket);
    void startCapture();
    void stopCapture();
    void teardownSocket();
    void fail(const QString& message);

    QWebSocket* m_socket = nullptr;
    QObject* m_audioInput = nullptr;
    QIODevice* m_audioDevice = nullptr;
    QTimer* m_fakePcmTimer = nullptr;
    QString m_deviceTag;
    QString m_channelTag;
    bool m_comInitialized = false;
    bool m_ready = false;
    bool m_stopped = false;
};

class RUSTCAM_CORE_API AudioTalkClient : public QObject {
    Q_OBJECT
public:
    enum class State {
        Idle = 0,
        Connecting = 1,
        Inviting = 2,
        Ready = 3,
        Error = 4
    };
    Q_ENUM(State)

    explicit AudioTalkClient(QObject* parent = nullptr);
    ~AudioTalkClient() override;

    void start(const QString& deviceTag, const QString& channelTag);
    void stop();

    State state() const { return m_state; }
    bool isActive() const { return m_state != State::Idle; }

signals:
    void stateChanged(rustcam::AudioTalkClient::State state);
    void errorOccurred(const QString& message);

private:
    QThread* m_thread = nullptr;
    AudioTalkWorker* m_worker = nullptr;
    State m_state = State::Idle;
};

}
