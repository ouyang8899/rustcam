#include "AudioTalkClient.h"
#include "ServerApi.h"
#include "ServerConfig.h"
#include "common/Logger.h"

#include <QtGlobal>
#include <QAbstractSocket>
#include <QIODevice>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMetaObject>
#include <QThread>
#include <QTimer>
#include <QUrl>
#include <QUrlQuery>
#include <QWebSocket>

#if QT_VERSION >= QT_VERSION_CHECK(6, 0, 0)
#include <QAudioSource>
#include <QMediaDevices>
#else
#include <QAudioDeviceInfo>
#include <QAudioFormat>
#include <QAudioInput>
#endif

#ifdef Q_OS_WIN
#include <windows.h>
#endif

namespace rustcam {

namespace {

enum WorkerState {
    StateIdle = 0,
    StateConnecting = 1,
    StateInviting = 2,
    StateReady = 3,
    StateError = 4,
};

QAudioFormat makeCaptureFormat(int channels)
{
    QAudioFormat format;
    format.setSampleRate(48000);
    format.setChannelCount(channels);
#if QT_VERSION >= QT_VERSION_CHECK(6, 0, 0)
    format.setSampleFormat(QAudioFormat::Int16);
#else
    format.setSampleSize(16);
    format.setCodec("audio/pcm");
    format.setByteOrder(QAudioFormat::LittleEndian);
    format.setSampleType(QAudioFormat::SignedInt);
#endif
    return format;
}

#if QT_VERSION >= QT_VERSION_CHECK(6, 0, 0)
QAudioFormat pickCaptureFormat(const QAudioDevice& device)
{
    QAudioFormat format = makeCaptureFormat(1);
    if (device.isFormatSupported(format)) {
        return format;
    }

    format = makeCaptureFormat(2);
    if (device.isFormatSupported(format)) {
        LOG_WARNING("AudioTalk", "麦克风不支持 48kHz 单声道，回退到 48kHz 立体声");
        return format;
    }

    format = device.preferredFormat();
    LOG_WARNING("AudioTalk", QString("麦克风不支持 48kHz，使用设备默认格式：%1Hz/%2ch")
        .arg(format.sampleRate()).arg(format.channelCount()));
    return format;
}
#endif

}

AudioTalkWorker::AudioTalkWorker(QObject* parent)
    : QObject(parent)
{
}

AudioTalkWorker::~AudioTalkWorker()
{
    m_stopped = true;
    stopCapture();
    if (m_socket) {
        m_socket->disconnect(this);
        m_socket->close();
        m_socket = nullptr;
    }
}

void AudioTalkWorker::start(const QString& deviceTag, const QString& channelTag)
{
    m_deviceTag = deviceTag;
    m_channelTag = channelTag;
    m_ready = false;
    m_stopped = false;

    stopCapture();
    teardownSocket();

    if (m_deviceTag.isEmpty() || m_channelTag.isEmpty()) {
        fail(QStringLiteral("缺少设备或通道信息"));
        return;
    }

    emit stateChanged(StateConnecting);
    requestTicket();
}

void AudioTalkWorker::requestTicket()
{
    if (m_stopped) {
        return;
    }

    // 取票是阻塞式 HTTP 请求，只能在工作线程里做；回调同样运行在本线程。
    ServerApi::instance()->post(
        QStringLiteral("/api/v1/ws/ticket"),
        QJsonObject(),
        [this](bool success, const QJsonObject& response, const QString& error) {
            if (m_stopped) {
                return;
            }
            const QString ticket = response.value("data").toObject().value("ticket").toString();
            if (!success || response.value("code").toInt() != 200 || ticket.isEmpty()) {
                fail(error.isEmpty() ? QStringLiteral("获取语音广播票据失败") : error);
                return;
            }
            connectWithTicket(ticket);
        });
}

void AudioTalkWorker::connectWithTicket(const QString& ticket)
{
    if (m_stopped) {
        return;
    }

    QUrl url(ServerConfig::instance()->serverUrl());
    url.setScheme(url.scheme() == "https" ? "wss" : "ws");
    url.setPath(QString("/ws/audio-talk/%1/%2").arg(m_deviceTag, m_channelTag));

    QUrlQuery query;
    query.addQueryItem(QStringLiteral("ticket"), ticket);
    url.setQuery(query);

    m_socket = new QWebSocket(QString(), QWebSocketProtocol::VersionLatest, this);
    connect(m_socket, &QWebSocket::connected, this, &AudioTalkWorker::onConnected);
    connect(m_socket, &QWebSocket::disconnected, this, &AudioTalkWorker::onDisconnected);
    connect(m_socket, &QWebSocket::textMessageReceived, this, &AudioTalkWorker::onTextMessage);
#if QT_VERSION >= QT_VERSION_CHECK(6, 0, 0)
    connect(m_socket, &QWebSocket::errorOccurred, this, &AudioTalkWorker::onSocketError);
#else
    connect(m_socket, QOverload<QAbstractSocket::SocketError>::of(&QWebSocket::error),
            this, &AudioTalkWorker::onSocketError);
#endif

    LOG_INFO("AudioTalk", QString("连接语音广播：%1").arg(url.toString(QUrl::RemoveQuery)));
    m_socket->open(url);
}

void AudioTalkWorker::onConnected()
{
    if (m_stopped) {
        return;
    }

    emit stateChanged(StateInviting);
    startCapture();
    if (m_socket) {
        m_socket->sendTextMessage(QStringLiteral("{\"action\":\"start_talk\"}"));
    }
}

void AudioTalkWorker::startCapture()
{
    stopCapture();

#ifdef Q_OS_WIN
    // WASAPI 依赖 COM，工作线程必须先初始化，否则打不开麦克风。
    if (!m_comInitialized) {
        const HRESULT hr = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
        m_comInitialized = (hr == S_OK || hr == S_FALSE);
        if (!m_comInitialized && hr != RPC_E_CHANGED_MODE) {
            LOG_WARNING("AudioTalk", QString("CoInitializeEx 失败：0x%1").arg(static_cast<uint>(hr), 8, 16, QLatin1Char('0')));
        }
    }
#endif

    // 测试钩子：没有麦克风（或被安全软件拦截）时用合成正弦波代替采集，
    // 便于验证客户端到服务端的音频链路。
    if (qEnvironmentVariableIsSet("RUSTCAM_AUDIO_TALK_FAKE_PCM")) {
        m_fakePcmTimer = new QTimer(this);
        m_fakePcmTimer->setInterval(100);
        connect(m_fakePcmTimer, &QTimer::timeout, this, &AudioTalkWorker::onFakePcmTick);
        m_fakePcmTimer->start();
        LOG_INFO("AudioTalk", "使用合成 PCM 测试音频");
        return;
    }

#if QT_VERSION >= QT_VERSION_CHECK(6, 0, 0)
    const QAudioDevice device = QMediaDevices::defaultAudioInput();
    if (device.isNull()) {
        fail(QStringLiteral("未找到可用的麦克风"));
        return;
    }
    const QAudioFormat format = pickCaptureFormat(device);
    auto* input = new QAudioSource(device, format, this);
    input->setBufferSize(static_cast<int>(format.bytesForDuration(100000)));
    m_audioInput = input;
    m_audioDevice = input->start();
    if (!m_audioDevice) {
        fail(QStringLiteral("无法打开麦克风"));
        return;
    }
    connect(m_audioDevice, &QIODevice::readyRead, this, &AudioTalkWorker::onAudioReadyRead);
    LOG_INFO("AudioTalk", QString("麦克风已打开：%1Hz/%2ch")
        .arg(format.sampleRate()).arg(format.channelCount()));
#else
    // 默认设备打不开时，逐个尝试其它输入设备，避免个别设备不支持目标格式导致整体失败。
    QList<QAudioDeviceInfo> candidates;
    const QAudioDeviceInfo defaultDevice = QAudioDeviceInfo::defaultInputDevice();
    if (!defaultDevice.isNull()) {
        candidates << defaultDevice;
    }
    const QList<QAudioDeviceInfo> allDevices = QAudioDeviceInfo::availableDevices(QAudio::AudioInput);
    for (const QAudioDeviceInfo& device : allDevices) {
        if (device == defaultDevice || device.supportedSampleRates().isEmpty()) {
            continue;
        }
        candidates << device;
    }

    if (candidates.isEmpty()) {
        fail(QStringLiteral("未找到可用的麦克风"));
        return;
    }

    for (const QAudioDeviceInfo& device : candidates) {
        QList<QAudioFormat> formats;
        formats << makeCaptureFormat(1) << makeCaptureFormat(2);
        const QAudioFormat nearest = device.nearestFormat(makeCaptureFormat(1));
        if (!nearest.isValid()) {
            continue;
        }
        formats << nearest;

        for (const QAudioFormat& format : formats) {
            auto* input = new QAudioInput(device, format, this);
            input->setBufferSize(static_cast<int>(format.bytesForDuration(100000)));
            QIODevice* dev = input->start();
            if (!dev) {
                LOG_WARNING("AudioTalk", QString("麦克风打开失败：%1 / %2Hz / %3ch")
                    .arg(device.deviceName()).arg(format.sampleRate()).arg(format.channelCount()));
                delete input;
                continue;
            }

            m_audioInput = input;
            m_audioDevice = dev;
            connect(dev, &QIODevice::readyRead, this, &AudioTalkWorker::onAudioReadyRead);
            LOG_INFO("AudioTalk", QString("麦克风已打开：%1 / %2Hz / %3ch")
                .arg(device.deviceName()).arg(format.sampleRate()).arg(format.channelCount()));
            return;
        }
    }

    fail(QStringLiteral("无法打开麦克风"));
#endif
}

void AudioTalkWorker::stopCapture()
{
    if (m_fakePcmTimer) {
        m_fakePcmTimer->stop();
        m_fakePcmTimer->deleteLater();
        m_fakePcmTimer = nullptr;
    }
    if (m_audioInput) {
#if QT_VERSION >= QT_VERSION_CHECK(6, 0, 0)
        static_cast<QAudioSource*>(m_audioInput)->stop();
#else
        static_cast<QAudioInput*>(m_audioInput)->stop();
#endif
        m_audioInput->deleteLater();
        m_audioInput = nullptr;
    }
    m_audioDevice = nullptr;
}

void AudioTalkWorker::teardownSocket()
{
    if (m_socket) {
        QWebSocket* socket = m_socket;
        m_socket = nullptr;
        socket->disconnect(this);
        socket->close();
        socket->deleteLater();
    }
}

void AudioTalkWorker::onAudioReadyRead()
{
    if (!m_audioDevice) {
        return;
    }

    const QByteArray data = m_audioDevice->readAll();
    if (data.isEmpty()) {
        return;
    }

    // 设备真正开始接收后再发流，避免把建链阶段的音频冲掉。
    if (!m_ready || !m_socket || m_socket->state() != QAbstractSocket::ConnectedState) {
        return;
    }

    m_socket->sendBinaryMessage(data);
}

void AudioTalkWorker::onFakePcmTick()
{
    if (!m_ready || !m_socket || m_socket->state() != QAbstractSocket::ConnectedState) {
        return;
    }

    // 48kHz 单声道 16bit，100ms 一包的正弦波。
    const int sampleRate = 48000;
    const int samples = sampleRate / 10;
    QByteArray pcm;
    pcm.reserve(samples * 2);
    for (int i = 0; i < samples; ++i) {
        const double phase = 2.0 * 3.14159265358979323846 * 440.0 * i / sampleRate;
        const qint16 value = static_cast<qint16>(sin(phase) * 8000.0);
        pcm.append(static_cast<char>(value & 0xFF));
        pcm.append(static_cast<char>((value >> 8) & 0xFF));
    }
    m_socket->sendBinaryMessage(pcm);
}

void AudioTalkWorker::onTextMessage(const QString& message)
{
    if (m_stopped) {
        return;
    }

    QJsonParseError parseError;
    const QJsonDocument doc = QJsonDocument::fromJson(message.toUtf8(), &parseError);
    if (parseError.error != QJsonParseError::NoError || !doc.isObject()) {
        return;
    }

    const QJsonObject obj = doc.object();
    const QString status = obj.value("status").toString();
    if (status == QLatin1String("ready")) {
        if (!m_ready) {
            m_ready = true;
            emit stateChanged(StateReady);
        }
    } else if (status == QLatin1String("error")) {
        fail(obj.value("message").toString(QStringLiteral("语音广播启动失败")));
    }
}

void AudioTalkWorker::onSocketError()
{
    if (m_stopped || !m_socket) {
        return;
    }
    fail(m_socket->errorString());
}

void AudioTalkWorker::onDisconnected()
{
    if (m_stopped) {
        return;
    }

    m_stopped = true;
    m_ready = false;
    stopCapture();
    teardownSocket();
    emit errorOccurred(QStringLiteral("语音广播连接已断开"));
    emit stateChanged(StateError);
}

void AudioTalkWorker::fail(const QString& message)
{
    if (m_stopped) {
        return;
    }

    m_stopped = true;
    m_ready = false;
    LOG_WARNING("AudioTalk", QString("语音广播失败：%1").arg(message));
    stopCapture();
    teardownSocket();
    emit errorOccurred(message);
    emit stateChanged(StateError);
}

void AudioTalkWorker::stop()
{
    m_stopped = true;
    m_ready = false;
    if (m_socket && m_socket->state() == QAbstractSocket::ConnectedState) {
        m_socket->sendTextMessage(QStringLiteral("{\"action\":\"stop_talk\"}"));
    }
    stopCapture();
    teardownSocket();
    emit stateChanged(StateIdle);
}

AudioTalkClient::AudioTalkClient(QObject* parent)
    : QObject(parent)
{
    qRegisterMetaType<AudioTalkClient::State>("AudioTalkClient::State");

    m_thread = new QThread(this);
    m_worker = new AudioTalkWorker;
    m_worker->moveToThread(m_thread);

    connect(m_thread, &QThread::finished, m_worker, &QObject::deleteLater);
    connect(m_worker, &AudioTalkWorker::stateChanged, this, [this](int state) {
        m_state = static_cast<State>(state);
        emit stateChanged(m_state);
    });
    connect(m_worker, &AudioTalkWorker::errorOccurred, this, &AudioTalkClient::errorOccurred);

    m_thread->start();
}

AudioTalkClient::~AudioTalkClient()
{
    if (m_worker && m_thread && m_thread->isRunning()) {
        // 先让工作线程释放麦克风和 WebSocket，再退出事件循环。
        QMetaObject::invokeMethod(m_worker, "stop", Qt::QueuedConnection);
    }
    if (m_thread) {
        m_thread->quit();
        m_thread->wait();
    }
}

void AudioTalkClient::start(const QString& deviceTag, const QString& channelTag)
{
    if (!m_worker) {
        return;
    }
    QMetaObject::invokeMethod(m_worker, "start", Qt::QueuedConnection,
                              Q_ARG(QString, deviceTag), Q_ARG(QString, channelTag));
}

void AudioTalkClient::stop()
{
    if (!m_worker) {
        return;
    }
    QMetaObject::invokeMethod(m_worker, "stop", Qt::QueuedConnection);
}

}
