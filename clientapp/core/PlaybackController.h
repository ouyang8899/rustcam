#pragma once

#include <QObject>
#include <QList>
#include <QMutex>
#include <functional>
#include "StreamSource.h"

namespace rustcam {

class RUSTCAM_CORE_API PlaybackGroup : public QObject {
    Q_OBJECT
public:
    explicit PlaybackGroup(int groupId, QObject* parent = nullptr);
    ~PlaybackGroup();

    int groupId() const { return m_groupId; }
    const QList<StreamSource*>& cells() const { return m_cells; }

    void addCell(StreamSource* cell);
    void removeCell(StreamSource* cell);
    void clearCells();

    void syncTo(int64_t baseTimeMs);
    void play();
    void pause();
    void stop();

signals:
    void cellAdded(StreamSource* cell);
    void cellRemoved(StreamSource* cell);

private:
    int m_groupId;
    QList<StreamSource*> m_cells;
};

class RUSTCAM_CORE_API PlaybackController : public QObject {
    Q_OBJECT
public:
    explicit PlaybackController(QObject* parent = nullptr);
    ~PlaybackController();

    int64_t baseTimeMs() const { return m_baseTimeMs; }
    const QList<PlaybackGroup*>& groups() const { return m_groups; }
    PlaybackGroup* defaultGroup() { return m_defaultGroup; }

    void setBaseTime(int64_t timeMs);
    void addGroup(PlaybackGroup* group);
    void removeGroup(PlaybackGroup* group);
    void removeGroup(int groupId);

    void play();
    void pause();
    void stop();
    void setSpeed(double speed);

    PlaybackGroup* createGroup();
    void destroyGroup(int groupId);

signals:
    void baseTimeChanged(int64_t timeMs);
    void groupAdded(PlaybackGroup* group);
    void groupRemoved(int groupId);

private:
    int64_t m_baseTimeMs = 0;
    QList<PlaybackGroup*> m_groups;
    PlaybackGroup* m_defaultGroup;
    int m_nextGroupId = 1;
    QMutex m_mutex;
};

}
