#include "SettingsWidget.h"
#include "core/SystemConfig.h"
#include "core/ConfigKeys.h"
#include "common/Logger.h"
#include <QGroupBox>
#include <QHBoxLayout>
#include <QVBoxLayout>
#include <QFormLayout>
#include <QPushButton>
#include <QLineEdit>
#include <QLabel>
#include <QTabWidget>
#include <QColorDialog>
#include <QFrame>
#include <QDialogButtonBox>

namespace rustcam {

SettingsWidget::SettingsWidget(QWidget* parent)
    : QWidget(parent)
    , m_tabWidget(nullptr)
    , m_mediaRateVisibleCheck(nullptr)
    , m_mediaRateAlignCombo(nullptr)
    , m_deviceNameVisibleCheck(nullptr)
    , m_deviceNameAlignCombo(nullptr)
    , m_timeVisibleCheck(nullptr)
    , m_timeAlignCombo(nullptr)
    , m_timeFormatCombo(nullptr)
    , m_watermarkVisibleCheck(nullptr)
    , m_watermarkTextEdit(nullptr)
    , m_osdFontSizeSpin(nullptr)
    , m_osdFontColorBtn(nullptr)
    , m_osdFontColorLabel(nullptr)
    , m_osdBgAlphaSpin(nullptr)
    , m_pluginOrderList(nullptr)
{
    setWindowTitle(tr("Settings"));
    setMinimumWidth(550);
    setMinimumHeight(450);

    QVBoxLayout* mainLayout = new QVBoxLayout(this);

    m_tabWidget = new QTabWidget(this);
    mainLayout->addWidget(m_tabWidget);

    setupOsdDisplayTab();
    setupOsdStyleTab();
    setupPluginTab();

    loadSettings();

    QPushButton* closeBtn = new QPushButton(tr("Close"), this);
    closeBtn->setFixedWidth(100);
    connect(closeBtn, &QPushButton::clicked, this, &QWidget::close);
    mainLayout->addWidget(closeBtn, 0, Qt::AlignRight);
}

SettingsWidget::~SettingsWidget() {
}

void SettingsWidget::loadSettings() {
    SystemConfig* cfg = SystemConfig::instance();

    m_mediaRateVisibleCheck->setChecked(cfg->getValue(CFG_MEDIA_RATE_VISIBLE, true));
    m_mediaRateAlignCombo->setCurrentIndex(cfg->getValue(CFG_MEDIA_RATE_ALIGN, 3));
    m_deviceNameVisibleCheck->setChecked(cfg->getValue(CFG_DEVICE_NAME_VISIBLE, true));
    m_deviceNameAlignCombo->setCurrentIndex(cfg->getValue(CFG_DEVICE_NAME_ALIGN, 0));
    m_timeVisibleCheck->setChecked(cfg->getValue(CFG_TIME_VISIBLE, true));
    m_timeAlignCombo->setCurrentIndex(cfg->getValue(CFG_TIME_ALIGN, 2));

    QString timeFormat = cfg->getValue(CFG_TIME_FORMAT, "hh:mm:ss").toString();
    int formatIndex = m_timeFormatCombo->findData(timeFormat);
    if (formatIndex >= 0) {
        m_timeFormatCombo->setCurrentIndex(formatIndex);
    }

    m_watermarkVisibleCheck->setChecked(cfg->getValue(CFG_WATERMARK_VISIBLE, false));
    m_watermarkTextEdit->setText(cfg->getValue(CFG_WATERMARK_TEXT, "").toString());
    m_osdFontSizeSpin->setValue(cfg->getValue(CFG_OSD_FONT_SIZE, 14));

    QString colorStr = cfg->getValue(CFG_OSD_FONT_COLOR, "#FFFFFFFF").toString();
    m_osdFontColor = QColor(colorStr);
    m_osdFontColorLabel->setStyleSheet(QString("background-color: %1;").arg(m_osdFontColor.name()));

    m_osdBgAlphaSpin->setValue(cfg->getValue(CFG_OSD_BG_ALPHA, 100));

    refreshPluginList();
}

void SettingsWidget::setupOsdDisplayTab() {
    QWidget* tab = new QWidget(this);
    QVBoxLayout* layout = new QVBoxLayout(tab);

    QGroupBox* rateGroup = new QGroupBox(tr("Media Rate"), tab);
    QFormLayout* rateLayout = new QFormLayout(rateGroup);

    m_mediaRateVisibleCheck = new QCheckBox(tr("Show media rate"), tab);
    rateLayout->addRow(tr("Display:"), m_mediaRateVisibleCheck);

    m_mediaRateAlignCombo = new QComboBox(tab);
    m_mediaRateAlignCombo->addItem(tr("Top Left"), 0);
    m_mediaRateAlignCombo->addItem(tr("Top Right"), 1);
    m_mediaRateAlignCombo->addItem(tr("Bottom Left"), 2);
    m_mediaRateAlignCombo->addItem(tr("Bottom Right"), 3);
    rateLayout->addRow(tr("Position:"), m_mediaRateAlignCombo);

    layout->addWidget(rateGroup);

    QGroupBox* nameGroup = new QGroupBox(tr("Device Name"), tab);
    QFormLayout* nameLayout = new QFormLayout(nameGroup);

    m_deviceNameVisibleCheck = new QCheckBox(tr("Show device name"), tab);
    nameLayout->addRow(tr("Display:"), m_deviceNameVisibleCheck);

    m_deviceNameAlignCombo = new QComboBox(tab);
    m_deviceNameAlignCombo->addItem(tr("Top Left"), 0);
    m_deviceNameAlignCombo->addItem(tr("Top Right"), 1);
    m_deviceNameAlignCombo->addItem(tr("Bottom Left"), 2);
    m_deviceNameAlignCombo->addItem(tr("Bottom Right"), 3);
    nameLayout->addRow(tr("Position:"), m_deviceNameAlignCombo);

    layout->addWidget(nameGroup);

    QGroupBox* timeGroup = new QGroupBox(tr("Time Display"), tab);
    QFormLayout* timeLayout = new QFormLayout(timeGroup);

    m_timeVisibleCheck = new QCheckBox(tr("Show time"), tab);
    timeLayout->addRow(tr("Display:"), m_timeVisibleCheck);

    m_timeAlignCombo = new QComboBox(tab);
    m_timeAlignCombo->addItem(tr("Top Left"), 0);
    m_timeAlignCombo->addItem(tr("Top Right"), 1);
    m_timeAlignCombo->addItem(tr("Bottom Left"), 2);
    m_timeAlignCombo->addItem(tr("Bottom Right"), 3);
    timeLayout->addRow(tr("Position:"), m_timeAlignCombo);

    m_timeFormatCombo = new QComboBox(tab);
    m_timeFormatCombo->addItem(tr("12:30:45"), "hh:mm:ss");
    m_timeFormatCombo->addItem(tr("12:30"), "hh:mm");
    m_timeFormatCombo->addItem(tr("2024-01-15"), "yyyy-MM-dd");
    m_timeFormatCombo->addItem(tr("2024-01-15 12:30:45"), "yyyy-MM-dd hh:mm:ss");
    m_timeFormatCombo->addItem(tr("01/15 12:30"), "MM/dd hh:mm");
    m_timeFormatCombo->addItem(tr("15 12:30"), "dd hh:mm");
    timeLayout->addRow(tr("Format:"), m_timeFormatCombo);

    layout->addWidget(timeGroup);

    QGroupBox* watermarkGroup = new QGroupBox(tr("Watermark"), tab);
    QFormLayout* watermarkLayout = new QFormLayout(watermarkGroup);

    m_watermarkVisibleCheck = new QCheckBox(tr("Show watermark"), tab);
    watermarkLayout->addRow(tr("Display:"), m_watermarkVisibleCheck);

    m_watermarkTextEdit = new QLineEdit(tab);
    m_watermarkTextEdit->setPlaceholderText(tr("Watermark text"));
    watermarkLayout->addRow(tr("Text:"), m_watermarkTextEdit);

    layout->addWidget(watermarkGroup);
    layout->addStretch();

    connect(m_mediaRateVisibleCheck, &QCheckBox::toggled, this, &SettingsWidget::onMediaRateVisibleChanged);
    connect(m_mediaRateAlignCombo, QOverload<int>::of(&QComboBox::currentIndexChanged), this, &SettingsWidget::onMediaRateAlignChanged);
    connect(m_deviceNameVisibleCheck, &QCheckBox::toggled, this, &SettingsWidget::onDeviceNameVisibleChanged);
    connect(m_deviceNameAlignCombo, QOverload<int>::of(&QComboBox::currentIndexChanged), this, &SettingsWidget::onDeviceNameAlignChanged);
    connect(m_timeVisibleCheck, &QCheckBox::toggled, this, &SettingsWidget::onTimeVisibleChanged);
    connect(m_timeAlignCombo, QOverload<int>::of(&QComboBox::currentIndexChanged), this, &SettingsWidget::onTimeAlignChanged);
    connect(m_timeFormatCombo, QOverload<int>::of(&QComboBox::currentIndexChanged), this, &SettingsWidget::onTimeFormatChanged);
    connect(m_watermarkVisibleCheck, &QCheckBox::toggled, this, &SettingsWidget::onWatermarkVisibleChanged);
    connect(m_watermarkTextEdit, &QLineEdit::textChanged, this, &SettingsWidget::onWatermarkTextChanged);

    m_tabWidget->addTab(tab, tr("OSD Display"));
}

void SettingsWidget::setupOsdStyleTab() {
    QWidget* tab = new QWidget(this);
    QVBoxLayout* layout = new QVBoxLayout(tab);

    QGroupBox* styleGroup = new QGroupBox(tr("OSD Style"), tab);
    QFormLayout* styleLayout = new QFormLayout(styleGroup);

    m_osdFontSizeSpin = new QSpinBox(tab);
    m_osdFontSizeSpin->setMinimum(8);
    m_osdFontSizeSpin->setMaximum(32);
    m_osdFontSizeSpin->setSuffix(tr(" px"));
    styleLayout->addRow(tr("Font Size:"), m_osdFontSizeSpin);

    QHBoxLayout* colorLayout = new QHBoxLayout();
    m_osdFontColorBtn = new QPushButton(tr("Select Color"), tab);
    m_osdFontColorLabel = new QLabel(tab);
    m_osdFontColorLabel->setFixedSize(60, 24);
    m_osdFontColorLabel->setFrameStyle(QFrame::Box | QFrame::Sunken);
    colorLayout->addWidget(m_osdFontColorBtn);
    colorLayout->addWidget(m_osdFontColorLabel);
    colorLayout->addStretch();
    styleLayout->addRow(tr("Font Color:"), colorLayout);

    m_osdBgAlphaSpin = new QSpinBox(tab);
    m_osdBgAlphaSpin->setMinimum(0);
    m_osdBgAlphaSpin->setMaximum(255);
    styleLayout->addRow(tr("Background Alpha:"), m_osdBgAlphaSpin);

    layout->addWidget(styleGroup);
    layout->addStretch();

    connect(m_osdFontSizeSpin, QOverload<int>::of(&QSpinBox::valueChanged), this, &SettingsWidget::onOsdFontSizeChanged);
    connect(m_osdFontColorBtn, &QPushButton::clicked, this, &SettingsWidget::onOsdFontColorClicked);
    connect(m_osdBgAlphaSpin, QOverload<int>::of(&QSpinBox::valueChanged), this, &SettingsWidget::onOsdBgAlphaChanged);

    m_tabWidget->addTab(tab, tr("OSD Style"));
}

void SettingsWidget::setupPluginTab() {
    QWidget* tab = new QWidget(this);
    QVBoxLayout* layout = new QVBoxLayout(tab);

    QGroupBox* orderGroup = new QGroupBox(tr("Plugin Order"), tab);
    QVBoxLayout* orderLayout = new QVBoxLayout(orderGroup);

    m_pluginOrderList = new QListWidget(tab);
    m_pluginOrderList->setDragDropMode(QListWidget::InternalMove);
    m_pluginOrderList->setSelectionMode(QListWidget::SingleSelection);
    orderLayout->addWidget(m_pluginOrderList);

    QHBoxLayout* btnLayout = new QHBoxLayout();
    QPushButton* upBtn = new QPushButton(tr("Move Up"), tab);
    QPushButton* downBtn = new QPushButton(tr("Move Down"), tab);
    btnLayout->addWidget(upBtn);
    btnLayout->addWidget(downBtn);
    btnLayout->addStretch();
    orderLayout->addLayout(btnLayout);

    QLabel* hintLabel = new QLabel(tr("Hint: Drag to reorder, order affects left navigation bar"), tab);
    hintLabel->setStyleSheet("color: gray; font-size: 11px;");
    orderLayout->addWidget(hintLabel);

    layout->addWidget(orderGroup);

    connect(m_pluginOrderList->model(), &QAbstractItemModel::rowsMoved, this, &SettingsWidget::onPluginOrderChanged);
    connect(upBtn, &QPushButton::clicked, this, &SettingsWidget::onPluginMoveUp);
    connect(downBtn, &QPushButton::clicked, this, &SettingsWidget::onPluginMoveDown);

    m_tabWidget->addTab(tab, tr("Plugin Management"));
}

void SettingsWidget::refreshPluginList() {
    m_pluginOrderList->clear();

    QStringList order = SystemConfig::instance()->getValue(CFG_PLUGIN_ORDER).toStringList();
    if (order.isEmpty()) {
        order = QStringList({"live_video", "alarm", "playback", "map", "settings"});
    }

    QMap<QString, QString> displayNames = {
        {"live_video", tr("Live Video")},
        {"alarm", tr("Alarm Center")},
        {"playback", tr("Playback")},
        {"map", tr("Map")},
        {"settings", tr("Settings")}
    };

    for (const QString& id : order) {
        QString displayName = displayNames.value(id, id);
        new QListWidgetItem(displayName, m_pluginOrderList);
    }
}

void SettingsWidget::onPluginOrderChanged() {
    QStringList order;
    for (int i = 0; i < m_pluginOrderList->count(); ++i) {
        QListWidgetItem* item = m_pluginOrderList->item(i);
        QString text = item->text();

        QMap<QString, QString> idMap = {
            {tr("Live Video"), "live_video"},
            {tr("Alarm Center"), "alarm"},
            {tr("Playback"), "playback"},
            {tr("Map"), "map"},
            {tr("Settings"), "settings"}
        };

        order.append(idMap.value(text, text));
    }

    SystemConfig::instance()->setValue(CFG_PLUGIN_ORDER, order);
}

void SettingsWidget::onPluginMoveUp() {
    int row = m_pluginOrderList->currentRow();
    if (row > 0) {
        QListWidgetItem* item = m_pluginOrderList->takeItem(row);
        m_pluginOrderList->insertItem(row - 1, item);
        m_pluginOrderList->setCurrentItem(item);
        onPluginOrderChanged();
    }
}

void SettingsWidget::onPluginMoveDown() {
    int row = m_pluginOrderList->currentRow();
    if (row < m_pluginOrderList->count() - 1) {
        QListWidgetItem* item = m_pluginOrderList->takeItem(row);
        m_pluginOrderList->insertItem(row + 1, item);
        m_pluginOrderList->setCurrentItem(item);
        onPluginOrderChanged();
    }
}

void SettingsWidget::onMediaRateVisibleChanged(bool checked) {
    SystemConfig::instance()->setValue(CFG_MEDIA_RATE_VISIBLE, checked);
}

void SettingsWidget::onMediaRateAlignChanged(int index) {
    SystemConfig::instance()->setValue(CFG_MEDIA_RATE_ALIGN, index);
}

void SettingsWidget::onDeviceNameVisibleChanged(bool checked) {
    SystemConfig::instance()->setValue(CFG_DEVICE_NAME_VISIBLE, checked);
}

void SettingsWidget::onDeviceNameAlignChanged(int index) {
    SystemConfig::instance()->setValue(CFG_DEVICE_NAME_ALIGN, index);
}

void SettingsWidget::onTimeVisibleChanged(bool checked) {
    SystemConfig::instance()->setValue(CFG_TIME_VISIBLE, checked);
}

void SettingsWidget::onTimeAlignChanged(int index) {
    SystemConfig::instance()->setValue(CFG_TIME_ALIGN, index);
}

void SettingsWidget::onTimeFormatChanged(int index) {
    QString format = m_timeFormatCombo->itemData(index).toString();
    SystemConfig::instance()->setValue(CFG_TIME_FORMAT, format);
}

void SettingsWidget::onWatermarkVisibleChanged(bool checked) {
    SystemConfig::instance()->setValue(CFG_WATERMARK_VISIBLE, checked);
}

void SettingsWidget::onWatermarkTextChanged(const QString& text) {
    SystemConfig::instance()->setValue(CFG_WATERMARK_TEXT, text);
}

void SettingsWidget::onOsdFontSizeChanged(int value) {
    SystemConfig::instance()->setValue(CFG_OSD_FONT_SIZE, value);
}

void SettingsWidget::onOsdFontColorClicked() {
    QColor color = QColorDialog::getColor(m_osdFontColor, this, tr("Select font color"));
    if (color.isValid()) {
        m_osdFontColor = color;
        m_osdFontColorLabel->setStyleSheet(QString("background-color: %1;").arg(color.name()));
        SystemConfig::instance()->setValue(CFG_OSD_FONT_COLOR, color.name(QColor::HexArgb));
    }
}

void SettingsWidget::onOsdBgAlphaChanged(int value) {
    SystemConfig::instance()->setValue(CFG_OSD_BG_ALPHA, value);
}

}
