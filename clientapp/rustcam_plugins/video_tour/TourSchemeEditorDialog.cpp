#include "TourSchemeEditorDialog.h"

#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QMessageBox>
#include <QPushButton>
#include <QSpinBox>
#include <QSplitter>
#include <QTabWidget>
#include <QTableWidget>
#include <QTimeEdit>
#include <QVBoxLayout>

#include "TourApi.h"
#include "core/DataManager.h"
#include "core/DeviceTree.h"

namespace rustcam {

TourSchemeEditorDialog::TourSchemeEditorDialog(TourApi* api, const QList<PlayerLayout>& layouts,
                                               QWidget* parent)
    : QDialog(parent)
    , m_api(api)
    , m_layouts(layouts)
{
    setWindowTitle(tr("编辑轮询方案"));
    resize(960, 640);
    setupUi();

    if (!DataManager::instance()->isLoaded()) {
        connect(DataManager::instance(), &DataManager::loadFinished, this,
                [this](bool success) {
                    if (success) {
                        m_tree->setNodes(DataManager::instance()->regionTree());
                    }
                });
        DataManager::instance()->loadAll();
    } else {
        m_tree->setNodes(DataManager::instance()->regionTree());
    }
}

void TourSchemeEditorDialog::setupUi()
{
    QVBoxLayout* root = new QVBoxLayout(this);

    QHBoxLayout* topRow = new QHBoxLayout();
    topRow->addWidget(new QLabel(tr("方案名称"), this));
    m_nameEdit = new QLineEdit(this);
    topRow->addWidget(m_nameEdit, 1);
    topRow->addWidget(new QLabel(tr("布局"), this));
    m_layoutCombo = new QComboBox(this);
    for (const PlayerLayout& layout : m_layouts) {
        m_layoutCombo->addItem(QStringLiteral("%1 (%2路)").arg(layout.name).arg(layout.layoutJson.size()),
                               layout.id);
    }
    topRow->addWidget(m_layoutCombo);
    m_layoutInfoLabel = new QLabel(this);
    topRow->addWidget(m_layoutInfoLabel);
    root->addLayout(topRow);

    QSplitter* splitter = new QSplitter(Qt::Horizontal, this);

    QTabWidget* trees = new QTabWidget(splitter);
    m_tree = new DeviceTree(trees);
    trees->addTab(m_tree, tr("地区/分组"));

    QWidget* queuePanel = new QWidget(splitter);
    QVBoxLayout* queueLayout = new QVBoxLayout(queuePanel);
    queueLayout->addWidget(new QLabel(tr("通道队列（双击树节点加入）"), queuePanel));
    m_queueList = new QListWidget(queuePanel);
    queueLayout->addWidget(m_queueList, 1);
    QHBoxLayout* queueButtons = new QHBoxLayout();
    QPushButton* addButton = new QPushButton(tr("加入"), queuePanel);
    QPushButton* removeButton = new QPushButton(tr("移除"), queuePanel);
    QPushButton* upButton = new QPushButton(tr("上移"), queuePanel);
    QPushButton* downButton = new QPushButton(tr("下移"), queuePanel);
    queueButtons->addWidget(addButton);
    queueButtons->addWidget(removeButton);
    queueButtons->addWidget(upButton);
    queueButtons->addWidget(downButton);
    queueLayout->addLayout(queueButtons);
    splitter->addWidget(queuePanel);
    splitter->setStretchFactor(1, 1);

    root->addWidget(splitter, 1);

    // 锁定设置
    QGroupBox* lockBox = new QGroupBox(tr("锁定窗口"), this);
    QVBoxLayout* lockLayout = new QVBoxLayout(lockBox);
    QHBoxLayout* lockRow = new QHBoxLayout();
    m_lockItemCombo = new QComboBox(lockBox);
    m_lockChannelCombo = new QComboBox(lockBox);
    QPushButton* addLockButton = new QPushButton(tr("添加锁定"), lockBox);
    QPushButton* removeLockButton = new QPushButton(tr("移除锁定"), lockBox);
    lockRow->addWidget(new QLabel(tr("窗口"), lockBox));
    lockRow->addWidget(m_lockItemCombo, 1);
    lockRow->addWidget(new QLabel(tr("通道"), lockBox));
    lockRow->addWidget(m_lockChannelCombo, 1);
    lockRow->addWidget(addLockButton);
    lockRow->addWidget(removeLockButton);
    lockLayout->addLayout(lockRow);
    m_lockTable = new QTableWidget(0, 3, lockBox);
    m_lockTable->setHorizontalHeaderLabels(QStringList() << tr("窗口") << tr("设备") << tr("通道"));
    m_lockTable->horizontalHeader()->setStretchLastSection(true);
    lockLayout->addWidget(m_lockTable);
    root->addWidget(lockBox);

    // 运行参数
    QGroupBox* paramBox = new QGroupBox(tr("运行参数"), this);
    QFormLayout* paramLayout = new QFormLayout(paramBox);
    m_dwellSpin = new QSpinBox(paramBox);
    m_dwellSpin->setRange(1000, 86400000);
    m_dwellSpin->setSingleStep(1000);
    m_dwellSpin->setSuffix(tr(" ms"));
    m_dwellSpin->setValue(10000);
    paramLayout->addRow(tr("每批展示时长"), m_dwellSpin);

    m_batchTimeoutSpin = new QSpinBox(paramBox);
    m_batchTimeoutSpin->setRange(500, 86400000);
    m_batchTimeoutSpin->setSingleStep(500);
    m_batchTimeoutSpin->setSuffix(tr(" ms"));
    m_batchTimeoutSpin->setValue(5000);
    paramLayout->addRow(tr("首帧等待上限"), m_batchTimeoutSpin);

    m_switchGapSpin = new QSpinBox(paramBox);
    m_switchGapSpin->setRange(0, 600000);
    m_switchGapSpin->setSingleStep(500);
    m_switchGapSpin->setSuffix(tr(" ms"));
    paramLayout->addRow(tr("批间间隔"), m_switchGapSpin);

    m_cycleGapSpin = new QSpinBox(paramBox);
    m_cycleGapSpin->setRange(0, 600000);
    m_cycleGapSpin->setSingleStep(500);
    m_cycleGapSpin->setSuffix(tr(" ms"));
    paramLayout->addRow(tr("轮次间隔"), m_cycleGapSpin);

    m_backfillSpin = new QSpinBox(paramBox);
    m_backfillSpin->setRange(0, 10);
    paramLayout->addRow(tr("单窗口补位上限"), m_backfillSpin);

    m_infiniteCheck = new QCheckBox(tr("无限次运行"), paramBox);
    m_maxCyclesSpin = new QSpinBox(paramBox);
    m_maxCyclesSpin->setRange(1, 100000);
    m_maxCyclesSpin->setValue(1);
    paramLayout->addRow(m_infiniteCheck, m_maxCyclesSpin);

    m_runWindowCheck = new QCheckBox(tr("限定运行时段"), paramBox);
    paramLayout->addRow(m_runWindowCheck);
    QHBoxLayout* timeRow = new QHBoxLayout();
    m_startTimeEdit = new QTimeEdit(QTime(0, 0), paramBox);
    m_endTimeEdit = new QTimeEdit(QTime(23, 59), paramBox);
    m_startTimeEdit->setDisplayFormat(QStringLiteral("HH:mm"));
    m_endTimeEdit->setDisplayFormat(QStringLiteral("HH:mm"));
    m_crossDayCheck = new QCheckBox(tr("跨天"), paramBox);
    timeRow->addWidget(new QLabel(tr("开始"), paramBox));
    timeRow->addWidget(m_startTimeEdit);
    timeRow->addWidget(new QLabel(tr("结束"), paramBox));
    timeRow->addWidget(m_endTimeEdit);
    timeRow->addWidget(m_crossDayCheck);
    paramLayout->addRow(timeRow);

    QHBoxLayout* weekdayRow = new QHBoxLayout();
    const QStringList weekdayNames = QStringList() << tr("一") << tr("二") << tr("三") << tr("四")
                                                  << tr("五") << tr("六") << tr("日");
    for (int i = 0; i < weekdayNames.size(); ++i) {
        QCheckBox* check = new QCheckBox(weekdayNames.at(i), paramBox);
        m_weekdayChecks.append(check);
        weekdayRow->addWidget(check);
    }
    weekdayRow->addStretch(1);
    paramLayout->addRow(tr("星期"), weekdayRow);

    m_safetySpin = new QSpinBox(paramBox);
    m_safetySpin->setRange(0, 1000000);
    m_safetySpin->setSuffix(tr(" 分钟"));
    paramLayout->addRow(tr("安全兜底时长（0 表示不设置）"), m_safetySpin);

    root->addWidget(paramBox);

    m_buttonBox = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    root->addWidget(m_buttonBox);

    connect(m_buttonBox, &QDialogButtonBox::accepted, this, &TourSchemeEditorDialog::onAccept);
    connect(m_buttonBox, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(m_layoutCombo, QOverload<int>::of(&QComboBox::currentIndexChanged), this,
            &TourSchemeEditorDialog::onLayoutChanged);
    connect(m_tree, QOverload<QVariantMap>::of(&DeviceTree::itemDoubleClicked), this,
            &TourSchemeEditorDialog::onChannelActivated);
    connect(addButton, &QPushButton::clicked, this, &TourSchemeEditorDialog::onAddChannel);
    connect(removeButton, &QPushButton::clicked, this, &TourSchemeEditorDialog::onRemoveChannel);
    connect(upButton, &QPushButton::clicked, this, &TourSchemeEditorDialog::onMoveChannelUp);
    connect(downButton, &QPushButton::clicked, this, &TourSchemeEditorDialog::onMoveChannelDown);
    connect(addLockButton, &QPushButton::clicked, this, &TourSchemeEditorDialog::onAddLock);
    connect(removeLockButton, &QPushButton::clicked, this, &TourSchemeEditorDialog::onRemoveLock);
    connect(m_infiniteCheck, &QCheckBox::toggled, this, &TourSchemeEditorDialog::onInfiniteToggled);
    connect(m_runWindowCheck, &QCheckBox::toggled, this, &TourSchemeEditorDialog::onRunWindowToggled);

    onInfiniteToggled(m_infiniteCheck->isChecked());
    onRunWindowToggled(m_runWindowCheck->isChecked());
    updateLayoutItems();
}

void TourSchemeEditorDialog::editScheme(const TourScheme& scheme)
{
    m_expectedVersion = scheme.version;
    m_schemeId = scheme.id;
    m_layoutId = scheme.layoutId;
    m_nameEdit->setText(scheme.name);

    const int layoutIndex = m_layoutCombo->findData(scheme.layoutId);
    if (layoutIndex >= 0) {
        m_layoutCombo->setCurrentIndex(layoutIndex);
    }

    m_dwellSpin->setValue(scheme.dwellMs);
    m_batchTimeoutSpin->setValue(scheme.batchTimeoutMs);
    m_switchGapSpin->setValue(scheme.switchGapMs);
    m_cycleGapSpin->setValue(scheme.cycleGapMs);
    m_backfillSpin->setValue(scheme.backfillLimit);
    m_infiniteCheck->setChecked(scheme.maxCycles <= 0);
    m_maxCyclesSpin->setValue(scheme.maxCycles > 0 ? scheme.maxCycles : 1);
    m_runWindowCheck->setChecked(scheme.hasRunWindow);
    if (scheme.hasRunWindow) {
        const QTime start = QTime::fromString(scheme.runWindow.start, QStringLiteral("HH:mm"));
        const QTime end = QTime::fromString(scheme.runWindow.end, QStringLiteral("HH:mm"));
        if (start.isValid()) {
            m_startTimeEdit->setTime(start);
        }
        if (end.isValid()) {
            m_endTimeEdit->setTime(end);
        }
        m_crossDayCheck->setChecked(scheme.runWindow.crossDay);
        for (int i = 0; i < m_weekdayChecks.size(); ++i) {
            m_weekdayChecks.at(i)->setChecked(scheme.runWindow.weekdays.contains(i + 1));
        }
    }
    m_safetySpin->setValue(scheme.safetyMaxDurationMs > 0
                               ? static_cast<int>(scheme.safetyMaxDurationMs / 60000)
                               : 0);

    m_queueList->clear();
    m_queue.clear();
    for (const TourChannel& channel : scheme.channels) {
        m_queue.append(channel);
        m_queueList->addItem(channel.name.isEmpty()
                                 ? channel.deviceTag + QLatin1Char('/') + channel.channelTag
                                 : channel.name);
    }

    m_locks.clear();
    for (const LockedSlot& slot : scheme.lockedSet) {
        m_locks.append(slot);
    }
    updateLayoutItems();
    updateLockChannels();
}

void TourSchemeEditorDialog::updateLayoutItems()
{
    m_lockItemCombo->clear();
    const int layoutId = m_layoutCombo->currentData().toInt();
    for (const PlayerLayout& layout : m_layouts) {
        if (layout.id != layoutId) {
            continue;
        }
        for (const LayoutItem& item : layout.layoutJson) {
            m_lockItemCombo->addItem(item.id, item.id);
        }
        m_layoutInfoLabel->setText(tr("窗口数：%1").arg(layout.layoutJson.size()));
        return;
    }
    m_layoutInfoLabel->setText(tr("未选择布局"));
}

void TourSchemeEditorDialog::updateLockChannels()
{
    m_lockChannelCombo->clear();
    for (int i = 0; i < m_queueList->count(); ++i) {
        QListWidgetItem* item = m_queueList->item(i);
        m_lockChannelCombo->addItem(item->text(), i);
    }
}

void TourSchemeEditorDialog::updateQueueWidgets()
{
    updateLockChannels();
}

void TourSchemeEditorDialog::collectChannels(QVector<TourChannel>* channels) const
{
    for (int i = 0; i < m_queue.size(); ++i) {
        TourChannel channel = m_queue.at(i);
        channel.sort = i;
        channels->append(channel);
    }
}

int TourSchemeEditorDialog::currentLayoutWidth() const
{
    const int layoutId = m_layoutCombo->currentData().toInt();
    for (const PlayerLayout& layout : m_layouts) {
        if (layout.id == layoutId) {
            return layout.layoutJson.size();
        }
    }
    return 0;
}

QStringList TourSchemeEditorDialog::currentLayoutItemIds() const
{
    QStringList ids;
    const int layoutId = m_layoutCombo->currentData().toInt();
    for (const PlayerLayout& layout : m_layouts) {
        if (layout.id == layoutId) {
            for (const LayoutItem& item : layout.layoutJson) {
                ids.append(item.id);
            }
        }
    }
    return ids;
}

void TourSchemeEditorDialog::onLayoutChanged()
{
    m_locks.clear();
    updateLayoutItems();
    updateLockChannels();
}

void TourSchemeEditorDialog::onChannelActivated(const QVariantMap& meta)
{
    if (meta.value(QStringLiteral("type")).toString() != QStringLiteral("channel")) {
        return;
    }
    const QString deviceTag = meta.value(QStringLiteral("device_tag")).toString();
    const QString channelTag = meta.value(QStringLiteral("channel_tag")).toString();
    if (deviceTag.isEmpty() || channelTag.isEmpty()) {
        return;
    }
    TourChannel channel;
    channel.deviceTag = deviceTag;
    channel.channelTag = channelTag;
    channel.name = meta.value(QStringLiteral("name")).toString();
    if (channel.name.isEmpty()) {
        channel.name = deviceTag + QLatin1Char('/') + channelTag;
    }
    for (const TourChannel& existing : m_queue) {
        if (existing.deviceTag == channel.deviceTag && existing.channelTag == channel.channelTag) {
            return;  // 已在队列中
        }
    }
    m_queue.append(channel);
    m_queueList->addItem(channel.name);
    updateQueueWidgets();
}

void TourSchemeEditorDialog::onAddChannel()
{
    QMessageBox::information(this, tr("加入通道"),
                             tr("请在左侧设备树中双击通道节点加入队列。"));
}

void TourSchemeEditorDialog::onRemoveChannel()
{
    const int row = m_queueList->currentRow();
    if (row < 0 || row >= m_queue.size()) {
        return;
    }
    m_queueList->takeItem(row);
    m_queue.remove(row);
    updateQueueWidgets();
}

void TourSchemeEditorDialog::onMoveChannelUp()
{
    const int row = m_queueList->currentRow();
    if (row <= 0 || row >= m_queue.size()) {
        return;
    }
    QListWidgetItem* item = m_queueList->takeItem(row);
    m_queueList->insertItem(row - 1, item);
    m_queueList->setCurrentRow(row - 1);
    m_queue.swapItemsAt(row, row - 1);
}

void TourSchemeEditorDialog::onMoveChannelDown()
{
    const int row = m_queueList->currentRow();
    if (row < 0 || row >= m_queue.size() - 1) {
        return;
    }
    QListWidgetItem* item = m_queueList->takeItem(row);
    m_queueList->insertItem(row + 1, item);
    m_queueList->setCurrentRow(row + 1);
    m_queue.swapItemsAt(row, row + 1);
}

void TourSchemeEditorDialog::onAddLock()
{
    const QString itemId = m_lockItemCombo->currentData().toString();
    const int channelIndex = m_lockChannelCombo->currentData().toInt();
    if (itemId.isEmpty() || channelIndex < 0 || channelIndex >= m_queue.size()) {
        return;
    }
    const TourChannel& channel = m_queue.at(channelIndex);
    for (const LockedSlot& slot : m_locks) {
        if (slot.layoutItemId == itemId) {
            QMessageBox::warning(this, tr("重复锁定"),
                                 tr("窗口 %1 已锁定。").arg(itemId));
            return;
        }
    }
    LockedSlot slot;
    slot.layoutItemId = itemId;
    slot.deviceTag = channel.deviceTag;
    slot.channelTag = channel.channelTag;
    m_locks.append(slot);

    m_lockTable->setRowCount(m_locks.size());
    for (int i = 0; i < m_locks.size(); ++i) {
        m_lockTable->setItem(i, 0, new QTableWidgetItem(m_locks.at(i).layoutItemId));
        m_lockTable->setItem(i, 1, new QTableWidgetItem(m_locks.at(i).deviceTag));
        m_lockTable->setItem(i, 2, new QTableWidgetItem(m_locks.at(i).channelTag));
    }
}

void TourSchemeEditorDialog::onRemoveLock()
{
    const int row = m_lockTable->currentRow();
    if (row < 0 || row >= m_locks.size()) {
        return;
    }
    m_locks.remove(row);
    m_lockTable->removeRow(row);
}

void TourSchemeEditorDialog::onInfiniteToggled(bool infinite)
{
    m_maxCyclesSpin->setEnabled(!infinite);
}

void TourSchemeEditorDialog::onRunWindowToggled(bool enabled)
{
    m_startTimeEdit->setEnabled(enabled);
    m_endTimeEdit->setEnabled(enabled);
    m_crossDayCheck->setEnabled(enabled);
    for (QCheckBox* check : m_weekdayChecks) {
        check->setEnabled(enabled);
    }
}

TourScheme TourSchemeEditorDialog::scheme() const
{
    TourScheme result;
    result.id = m_schemeId;
    result.name = m_nameEdit->text().trimmed();
    result.version = m_expectedVersion;
    result.layoutId = m_layoutCombo->currentData().toInt();
    collectChannels(&result.channels);
    result.lockedSet = m_locks;
    result.dwellMs = m_dwellSpin->value();
    result.batchTimeoutMs = m_batchTimeoutSpin->value();
    result.switchGapMs = m_switchGapSpin->value();
    result.cycleGapMs = m_cycleGapSpin->value();
    result.backfillLimit = m_backfillSpin->value();
    result.maxCycles = m_infiniteCheck->isChecked() ? 0 : m_maxCyclesSpin->value();
    result.hasRunWindow = m_runWindowCheck->isChecked();
    if (result.hasRunWindow) {
        result.runWindow.start = m_startTimeEdit->time().toString(QStringLiteral("HH:mm"));
        result.runWindow.end = m_endTimeEdit->time().toString(QStringLiteral("HH:mm"));
        result.runWindow.crossDay = m_crossDayCheck->isChecked();
        for (int i = 0; i < m_weekdayChecks.size(); ++i) {
            if (m_weekdayChecks.at(i)->isChecked()) {
                result.runWindow.weekdays.append(i + 1);
            }
        }
    }
    result.safetyMaxDurationMs = static_cast<qint64>(m_safetySpin->value()) * 60000;
    return result;
}

void TourSchemeEditorDialog::onAccept()
{
    TourScheme candidate = scheme();
    QString error;
    if (!TourSchemeCodec::validate(candidate, currentLayoutWidth(), currentLayoutItemIds(), &error)) {
        QMessageBox::warning(this, tr("校验失败"), error);
        return;
    }
    QDialog::accept();
}

} // namespace rustcam
