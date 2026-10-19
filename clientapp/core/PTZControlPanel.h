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

    void setEnabled(bool enabled);
    bool isEnabled() const { return m_enabled; }
    void loadPresets(const QString& itemId);

signals:
    void ptzCommand(const QString& command, const QVariant& value = QVariant());

public slots:
    void onDeviceSelected(const QString& deviceId, const QString& channelId);

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

private:
    void setupConnections();

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
};

}
