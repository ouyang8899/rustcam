#pragma once

#include <QWidget>
#include <QLabel>
#include <QString>
#include <QList>
#include <QResizeEvent>
#include "CoreExport.h"
#include "StreamSource.h"

namespace rustcam {

class ReplayPlayer;
class PlaybackController;
class PlaybackGroup;

class RUSTCAM_CORE_API PlaybackCell : public QWidget {
    Q_OBJECT
public:
    explicit PlaybackCell(QWidget* parent = nullptr);
    ~PlaybackCell();

    int cellId() const { return m_cellId; }
    void setCellId(int id) { m_cellId = id; }

    const QString& deviceId() const { return m_deviceId; }

    void bindDevice(const QString& deviceId);
    void loadClips(const QList<Clip>& clips);
    void setController(PlaybackController* controller);
    void setGroup(PlaybackGroup* group);

    PlaybackController* controller() const { return m_controller; }
    PlaybackGroup* group() const { return m_group; }
    StreamSource* source() const { return m_source; }

    void setSelected(bool selected);

    void showNoRecording(int64_t timeMs);

signals:
    void clicked();
    void cellIdChanged(int oldId, int newId);
    void deviceBound(const QString& deviceId);

protected:
    void mousePressEvent(QMouseEvent* event) override;
    void resizeEvent(QResizeEvent* event) override;
    void paintEvent(QPaintEvent* event) override;

private:
    int m_cellId = -1;
    QString m_deviceId;
    ReplayPlayer* m_player;
    StreamSource* m_source;
    PlaybackController* m_controller;
    PlaybackGroup* m_group;
    QLabel* m_noRecordingLabel;
    bool m_selected = false;
};

}
