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
#include "TourLayoutPicker.h"
#include "TourTypes.h"
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
    resize(1080, 720);
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

    // 顶部常驻：方案名称与所属布局。
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

    m_tabs = new QTabWidget(this);
    root->addWidget(m_tabs, 1);

    setupQueueTab();
    setupLocalDisplayTab();
    setupParamTab();

    m_buttonBox = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    root->addWidget(m_buttonBox);

    connect(m_buttonBox, &QDialogButtonBox::accepted, this, &TourSchemeEditorDialog::onAccept);
    connect(m_buttonBox, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(m_layoutCombo, QOverload<int>::of(&QComboBox::currentIndexChanged), this,
            &TourSchemeEditorDialog::onLayoutChanged);

    onInfiniteToggled(m_infiniteCheck->isChecked());
    onRunWindowToggled(m_runWindowCheck->isChecked());
    updateLayoutItems();
}

void TourSchemeEditorDialog::setupQueueTab()
{
    QWidget* tab = new QWidget(m_tabs);
    QHBoxLayout* tabLayout = new QHBoxLayout(tab);
    tabLayout->setContentsMargins(0, 0, 0, 0);

    QSplitter* splitter = new QSplitter(Qt::Horizontal, tab);

    QTabWidget* trees = new QTabWidget(splitter);
    m_tree = new DeviceTree(trees);
    trees->addTab(m_tree, tr("地区/分组"));
    splitter->addWidget(trees);

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

    tabLayout->addWidget(splitter);

    connect(m_tree, QOverload<QVariantMap>::of(&DeviceTree::itemDoubleClicked), this,
            &TourSchemeEditorDialog::onChannelActivated);
    connect(addButton, &QPushButton::clicked, this, &TourSchemeEditorDialog::onAddChannel);
    connect(removeButton, &QPushButton::clicked, this, &TourSchemeEditorDialog::onRemoveChannel);
    connect(upButton, &QPushButton::clicked, this, &TourSchemeEditorDialog::onMoveChannelUp);
    connect(downButton, &QPushButton::clicked, this, &TourSchemeEditorDialog::onMoveChannelDown);

    m_tabs->addTab(tab, tr("通道队列"));
}

void TourSchemeEditorDialog::setupLocalDisplayTab()
{
    QWidget* tab = new QWidget(m_tabs);
    QVBoxLayout* tabLayout = new QVBoxLayout(tab);
    tabLayout->setContentsMargins(0, 0, 0, 0);

    QSplitter* splitter = new QSplitter(Qt::Horizontal, tab);

    // 左：布局缩略图，直接点选窗口。
    QWidget* pickerPanel = new QWidget(splitter);
    QVBoxLayout* pickerLayout = new QVBoxLayout(pickerPanel);
    pickerLayout->addWidget(new QLabel(tr("点击布局中的窗口进行选择"), pickerPanel));
    m_layoutPicker = new TourLayoutPicker(pickerPanel);
    pickerLayout->addWidget(m_layoutPicker, 1);
    splitter->addWidget(pickerPanel);

    // 右：通道与操作。
    QWidget* controlPanel = new QWidget(splitter);
    QVBoxLayout* controlLayout = new QVBoxLayout(controlPanel);
    controlLayout->addWidget(new QLabel(tr("通道"), controlPanel));
    m_lockChannelCombo = new QComboBox(controlPanel);
    controlLayout->addWidget(m_lockChannelCombo);
    m_assignLockButton = new QPushButton(tr("设为局部展示"), controlPanel);
    m_removeLockButton = new QPushButton(tr("取消局部展示"), controlPanel);
    controlLayout->addWidget(m_assignLockButton);
    controlLayout->addWidget(m_removeLockButton);
    m_lockHintLabel = new QLabel(controlPanel);
    m_lockHintLabel->setWordWrap(true);
    m_lockHintLabel->setStyleSheet("color: gray; font-size: 11px;");
    m_lockHintLabel->setText(tr("请在左侧布局中点击选择窗口。"));
    controlLayout->addWidget(m_lockHintLabel);
    controlLayout->addStretch(1);
    m_removeLockButton->setEnabled(false);
    splitter->addWidget(controlPanel);
    splitter->setStretchFactor(0, 1);

    tabLayout->addWidget(splitter, 1);

    // 下：局部展示清单。
    m_lockTable = new QTableWidget(0, 2, tab);
    m_lockTable->setHorizontalHeaderLabels(QStringList() << tr("窗口") << tr("通道"));
    m_lockTable->horizontalHeader()->setStretchLastSection(true);
    m_lockTable->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_lockTable->setEditTriggers(QAbstractItemView::NoEditTriggers);
    m_lockTable->setMaximumHeight(160);
    tabLayout->addWidget(m_lockTable);

    m_tabs->addTab(tab, tr("局部展示"));

    connect(m_layoutPicker, &TourLayoutPicker::itemSelected, this,
            &TourSchemeEditorDialog::onLayoutItemSelected);
    connect(m_assignLockButton, &QPushButton::clicked, this, &TourSchemeEditorDialog::onAssignLock);
    connect(m_removeLockButton, &QPushButton::clicked, this, &TourSchemeEditorDialog::onRemoveLock);
    connect(m_lockTable, &QTableWidget::itemSelectionChanged, this,
            &TourSchemeEditorDialog::onLockTableSelectionChanged);
}

void TourSchemeEditorDialog::setupParamTab()
{
    QWidget* tab = new QWidget(m_tabs);
    QVBoxLayout* tabLayout = new QVBoxLayout(tab);
    tabLayout->setContentsMargins(0, 0, 0, 0);

    QGroupBox* paramBox = new QGroupBox(tr("运行参数"), tab);
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

    tabLayout->addWidget(paramBox);
    tabLayout->addStretch(1);

    connect(m_infiniteCheck, &QCheckBox::toggled, this, &TourSchemeEditorDialog::onInfiniteToggled);
    connect(m_runWindowCheck, &QCheckBox::toggled, this, &TourSchemeEditorDialog::onRunWindowToggled);

    m_tabs->addTab(tab, tr("运行参数"));
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
    refreshLockWidgets();
}

void TourSchemeEditorDialog::updateLayoutItems()
{
    const int layoutId = m_layoutCombo->currentData().toInt();
    for (const PlayerLayout& layout : m_layouts) {
        if (layout.id != layoutId) {
            continue;
        }
        m_layoutInfoLabel->setText(tr("窗口数：%1").arg(layout.layoutJson.size()));
        if (m_layoutPicker) {
            m_layoutPicker->setLayout(layout);
        }
        refreshLockWidgets();
        return;
    }
    m_layoutInfoLabel->setText(tr("未选择布局"));
    if (m_layoutPicker) {
        m_layoutPicker->setLayout(PlayerLayout());
    }
}

void TourSchemeEditorDialog::updateLockChannels()
{
    m_lockChannelCombo->clear();
    for (const TourChannel& channel : m_queue) {
        const QString name = channel.name.isEmpty()
                                 ? channel.deviceTag + QLatin1Char('/') + channel.channelTag
                                 : channel.name;
        m_lockChannelCombo->addItem(name, channel.ref().key());
    }
}

void TourSchemeEditorDialog::updateQueueWidgets()
{
    updateLockChannels();
}

QHash<QString, QString> TourSchemeEditorDialog::lockChannelNames() const
{
    QHash<QString, QString> names;
    for (const TourChannel& channel : m_queue) {
        names.insert(channel.ref().key(),
                     channel.name.isEmpty()
                         ? channel.deviceTag + QLatin1Char('/') + channel.channelTag
                         : channel.name);
    }
    return names;
}

QString TourSchemeEditorDialog::layoutItemName(const QString& layoutItemId) const
{
    const int layoutId = m_layoutCombo->currentData().toInt();
    for (const PlayerLayout& layout : m_layouts) {
        if (layout.id != layoutId) {
            continue;
        }
        for (const LayoutItem& item : layout.layoutJson) {
            if (item.id == layoutItemId) {
                return layoutItemDisplayName(item);
            }
        }
    }
    return layoutItemId;
}

QString TourSchemeEditorDialog::channelName(const LockedSlot& slot) const
{
    for (const TourChannel& channel : m_queue) {
        if (channel.deviceTag == slot.deviceTag && channel.channelTag == slot.channelTag) {
            return channel.name.isEmpty()
                       ? channel.deviceTag + QLatin1Char('/') + channel.channelTag
                       : channel.name;
        }
    }
    return slot.deviceTag + QLatin1Char('/') + slot.channelTag;
}

void TourSchemeEditorDialog::refreshLockWidgets()
{
    if (!m_layoutPicker) {
        return;
    }
    m_layoutPicker->setLocks(m_locks, lockChannelNames());

    m_lockTable->setRowCount(m_locks.size());
    for (int i = 0; i < m_locks.size(); ++i) {
        const LockedSlot& slot = m_locks.at(i);
        m_lockTable->setItem(i, 0, new QTableWidgetItem(layoutItemName(slot.layoutItemId)));
        m_lockTable->setItem(i, 1, new QTableWidgetItem(channelName(slot)));
    }
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
    refreshLockWidgets();
    if (m_lockHintLabel) {
        m_lockHintLabel->setText(tr("请在左侧布局中点击选择窗口。"));
    }
    if (m_removeLockButton) {
        m_removeLockButton->setEnabled(false);
    }
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

void TourSchemeEditorDialog::onLayoutItemSelected(const QString& layoutItemId)
{
    if (layoutItemId.isEmpty()) {
        m_lockHintLabel->setText(tr("请在左侧布局中点击选择窗口。"));
        return;
    }

    const QString itemName = layoutItemName(layoutItemId);
    QString lockedChannel;
    for (const LockedSlot& slot : m_locks) {
        if (slot.layoutItemId == layoutItemId) {
            lockedChannel = channelName(slot);
            break;
        }
    }

    if (lockedChannel.isEmpty()) {
        m_lockHintLabel->setText(tr("已选窗口：%1，尚未设为局部展示。").arg(itemName));
        m_removeLockButton->setEnabled(false);
    } else {
        m_lockHintLabel->setText(tr("已选窗口：%1（局部展示：%2）").arg(itemName, lockedChannel));
        m_removeLockButton->setEnabled(true);
    }

    // 清单里已有该窗口时同步选中，方便直接取消局部展示。
    for (int i = 0; i < m_locks.size(); ++i) {
        if (m_locks.at(i).layoutItemId == layoutItemId) {
            m_lockTable->selectRow(i);
            break;
        }
    }
}

void TourSchemeEditorDialog::onLockTableSelectionChanged()
{
    const int row = m_lockTable->currentRow();
    if (row < 0 || row >= m_locks.size()) {
        return;
    }
    const QString itemId = m_locks.at(row).layoutItemId;
    if (m_layoutPicker && m_layoutPicker->selectedItemId() != itemId) {
        m_layoutPicker->selectItem(itemId);
        onLayoutItemSelected(itemId);
    }
}

void TourSchemeEditorDialog::onAssignLock()
{
    const QString itemId = m_layoutPicker ? m_layoutPicker->selectedItemId() : QString();
    if (itemId.isEmpty()) {
        QMessageBox::information(this, tr("设为局部展示"),
                                 tr("请先在布局缩略图中点击选择要固定的窗口。"));
        return;
    }

    const QString key = m_lockChannelCombo->currentData().toString();
    if (key.isEmpty()) {
        QMessageBox::information(this, tr("设为局部展示"),
                                 tr("请先在通道队列里选择一路通道。"));
        return;
    }

    for (const LockedSlot& slot : m_locks) {
        if (slot.layoutItemId == itemId) {
            QMessageBox::warning(this, tr("重复锁定"),
                                 tr("窗口 %1 已经是局部展示。").arg(layoutItemName(itemId)));
            return;
        }
    }

    TourChannel selected;
    bool found = false;
    for (const TourChannel& channel : m_queue) {
        if (channel.ref().key() == key) {
            selected = channel;
            found = true;
            break;
        }
    }
    if (!found) {
        return;
    }

    LockedSlot slot;
    slot.layoutItemId = itemId;
    slot.deviceTag = selected.deviceTag;
    slot.channelTag = selected.channelTag;
    m_locks.append(slot);

    refreshLockWidgets();
    onLayoutItemSelected(itemId);
}

void TourSchemeEditorDialog::onRemoveLock()
{
    const QString itemId = m_layoutPicker ? m_layoutPicker->selectedItemId() : QString();
    if (itemId.isEmpty()) {
        return;
    }
    for (int i = 0; i < m_locks.size(); ++i) {
        if (m_locks.at(i).layoutItemId == itemId) {
            m_locks.remove(i);
            refreshLockWidgets();
            onLayoutItemSelected(itemId);
            return;
        }
    }
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
