#pragma once

#include <QWidget>
#include <QToolButton>
#include <QPushButton>
#include <QLabel>
#include <QFrame>
#include <QListView>
#include <QSignalMapper>
#include <QVariant>
#include <QJsonObject>
#include <QJsonArray>
#include <QStringListModel>
#include "CoreExport.h"
#include "EventBus.h"
#include "ui_PTZControlPanel.h"

namespace rustcam {

class RUSTCAM_CORE_API PTZControlPanel : public QWidget {
    Q_OBJECT
public:
    explicit PTZControlPanel(QWidget* parent = nullptr);
    ~PTZControlPanel();

    /// 设置当前 PTZ 控制的通道；device_tag / channel_tag 任一为空表示没有目标。
    void setTarget(const QString& deviceTag, const QString& channelTag);
    /// 当前 PTZ 目标的设备标识
    QString deviceId() const { return m_deviceId; }
    /// 当前 PTZ 目标的通道标识
    QString channelId() const { return m_channelId; }
    /// 重新拉取当前目标的预置位
    void reloadPresets();
    /// 启用/禁用面板上的操作按钮
    void setControlsEnabled(bool enabled);
    bool controlsEnabled() const { return m_enabled; }

signals:
    void ptzCommand(const QString& command, const QVariant& value = QVariant());

private slots:
    void toggleCollapsed();
    void togglePresetList();
    void onDirectionClicked(const QString& direction);
    void onCenterClicked();
    void onPresetClicked(const QModelIndex& index);
    void onAuxFunctionClicked(const QString& function);
    void onAddPreset();
    void onRemovePreset();
    void onVideoCellSelected(const QString& topic, EventDataPtr data);
    void onVideoChannelPlayed(const QString& topic, EventDataPtr data);

private:
    void setupConnections();
    void loadPresets(const QString& deviceTag, const QString& channelTag);

    Ui::PTZControlPanel ui;
    bool m_collapsed = false;
    bool m_presetListCollapsed = true;
    bool m_enabled = false;
    QString m_deviceId;
    QString m_channelId;

    QStringListModel* m_presetModel;
    QSignalMapper* m_directionMapper = nullptr;
    QSignalMapper* m_presetMapper = nullptr;
    QSignalMapper* m_auxMapper = nullptr;
    QString m_videoCellSubscription;
    QString m_videoChannelSubscription;
};

}
