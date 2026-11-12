#pragma once

#include "TourScheme.h"

#include <QDialog>
#include <QHash>
#include <QList>
#include <QVector>

#include "PlayerLayout.h"

class QCheckBox;
class QComboBox;
class QDialogButtonBox;
class QLabel;
class QLineEdit;
class QListWidget;
class QPushButton;
class QSpinBox;
class QTableWidget;
class QTabWidget;
class QTimeEdit;

namespace rustcam {

class DeviceTree;
class TourApi;
class TourLayoutPicker;

/// 方案编辑弹窗：布局、通道队列、局部展示与运行参数，分三页签。
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
    void onAssignLock();
    void onRemoveLock();
    void onLayoutItemSelected(const QString& layoutItemId);
    void onLockTableSelectionChanged();
    void onAccept();
    void onInfiniteToggled(bool infinite);
    void onRunWindowToggled(bool enabled);

private:
    void setupUi();
    void setupQueueTab();
    void setupLocalDisplayTab();
    void setupParamTab();

    void updateLayoutItems();
    void updateLockChannels();
    void updateQueueWidgets();
    void collectChannels(QVector<TourChannel>* channels) const;
    int currentLayoutWidth() const;
    QStringList currentLayoutItemIds() const;

    /// 局部展示：刷新缩略图与清单
    void refreshLockWidgets();
    /// 局部展示：窗口的可读名称
    QString layoutItemName(const QString& layoutItemId) const;
    /// 局部展示：通道的可读名称
    QString channelName(const LockedSlot& slot) const;
    /// 局部展示：通道名表（键为 TourChannelRef::key()）
    QHash<QString, QString> lockChannelNames() const;

    TourApi* m_api = nullptr;
    QList<PlayerLayout> m_layouts;

    QLineEdit* m_nameEdit = nullptr;
    QComboBox* m_layoutCombo = nullptr;
    QLabel* m_layoutInfoLabel = nullptr;
    QTabWidget* m_tabs = nullptr;
    DeviceTree* m_tree = nullptr;
    QListWidget* m_queueList = nullptr;
    TourLayoutPicker* m_layoutPicker = nullptr;
    QComboBox* m_lockChannelCombo = nullptr;
    QPushButton* m_assignLockButton = nullptr;
    QPushButton* m_removeLockButton = nullptr;
    QLabel* m_lockHintLabel = nullptr;
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
