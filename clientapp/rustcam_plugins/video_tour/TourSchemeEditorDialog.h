#pragma once

#include "TourScheme.h"

#include <QDialog>
#include <QList>
#include <QVector>

#include "PlayerLayout.h"

class QCheckBox;
class QComboBox;
class QDialogButtonBox;
class QLabel;
class QLineEdit;
class QListWidget;
class QSpinBox;
class QTableWidget;
class QTimeEdit;

namespace rustcam {

class DeviceTree;
class TourApi;

/// 方案编辑弹窗：布局、通道队列、锁定与运行参数。
class TourSchemeEditorDialog : public QDialog {
    Q_OBJECT
public:
    TourSchemeEditorDialog(TourApi* api, const QList<PlayerLayout>& layouts,
                           QWidget* parent = nullptr);

    /// 载入方案并显示。
    void editScheme(const TourScheme& scheme);
    /// 当前编辑后的方案。
    TourScheme scheme() const;
    /// 打开时的版本号，保存时作为 expectedVersion。
    int expectedVersion() const { return m_expectedVersion; }

private slots:
    void onLayoutChanged();
    void onChannelActivated(const QVariantMap& meta);
    void onAddChannel();
    void onRemoveChannel();
    void onMoveChannelUp();
    void onMoveChannelDown();
    void onAddLock();
    void onRemoveLock();
    void onAccept();
    void onInfiniteToggled(bool infinite);
    void onRunWindowToggled(bool enabled);

private:
    void setupUi();
    void updateLayoutItems();
    void updateLockChannels();
    void updateQueueWidgets();
    void collectChannels(QVector<TourChannel>* channels) const;
    int currentLayoutWidth() const;
    QStringList currentLayoutItemIds() const;

    TourApi* m_api = nullptr;
    QList<PlayerLayout> m_layouts;

    QLineEdit* m_nameEdit = nullptr;
    QComboBox* m_layoutCombo = nullptr;
    QLabel* m_layoutInfoLabel = nullptr;
    DeviceTree* m_tree = nullptr;
    QListWidget* m_queueList = nullptr;
    QComboBox* m_lockItemCombo = nullptr;
    QComboBox* m_lockChannelCombo = nullptr;
    QTableWidget* m_lockTable = nullptr;
    QSpinBox* m_dwellSpin = nullptr;
    QSpinBox* m_batchTimeoutSpin = nullptr;
    QSpinBox* m_switchGapSpin = nullptr;
    QSpinBox* m_cycleGapSpin = nullptr;
    QSpinBox* m_maxCyclesSpin = nullptr;
    QCheckBox* m_infiniteCheck = nullptr;
    QCheckBox* m_runWindowCheck = nullptr;
    QTimeEdit* m_startTimeEdit = nullptr;
    QTimeEdit* m_endTimeEdit = nullptr;
    QCheckBox* m_crossDayCheck = nullptr;
    QVector<QCheckBox*> m_weekdayChecks;
    QSpinBox* m_safetySpin = nullptr;
    QSpinBox* m_backfillSpin = nullptr;
    QDialogButtonBox* m_buttonBox = nullptr;

    QVector<LockedSlot> m_locks;
    QVector<TourChannel> m_queue;
    qint64 m_schemeId = 0;
    int m_expectedVersion = 0;
    int m_layoutId = 0;
};

} // namespace rustcam
