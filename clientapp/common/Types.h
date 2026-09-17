#pragma once

#include <QString>
#include <QVariantMap>
#include <QObject>

namespace rustcam {

struct DeviceInfo {
    QString id;
    QString name;
    QString type;
    QString status;
    double lat = 0.0;
    double lon = 0.0;
};

struct AlarmInfo {
    QString id;
    QString deviceId;
    QString type;
    int level = 0;
    double lat = 0.0;
    double lon = 0.0;
    QString time;
    QString status;
};

struct DroneTelemetry {
    QString droneId;
    double lat = 0.0;
    double lon = 0.0;
    double alt = 0.0;
    double speed = 0.0;
    double battery = 100.0;
    double heading = 0.0;
    QString flightMode;
    bool isConnected = false;
};

enum class DroneState {
    Disconnected,
    Connecting,
    Connected,
    Arming,
    Armed,
    Flying,
    Returning,
    Landing,
    Emergency
};

enum class VideoStreamStatus {
    Stopped,
    Connecting,
    Playing,
    Error
};

struct VideoStreamInfo {
    QString url;
    QString cellId;
    QString title;
    int width = 0;
    int height = 0;
    VideoStreamStatus status = VideoStreamStatus::Stopped;
};

}

Q_DECLARE_METATYPE(rustcam::DeviceInfo)
Q_DECLARE_METATYPE(rustcam::AlarmInfo)
Q_DECLARE_METATYPE(rustcam::DroneTelemetry)
Q_DECLARE_METATYPE(rustcam::DroneState)
Q_DECLARE_METATYPE(rustcam::VideoStreamStatus)
