#include "PTZControlPanel.h"
#include "common/Logger.h"
#include "ServerApi.h"

namespace rustcam {

PTZControlPanel::PTZControlPanel(QWidget* parent)
    : QWidget(parent)
    , m_directionMapper(nullptr)
    , m_presetMapper(nullptr)
    , m_auxMapper(nullptr)
    , m_presetModel(new QStringListModel(this))
    , m_presetListCollapsed(true)
{
    ui.setupUi(this);

    QToolButton* dirBtns[8] = {ui.btnUpLeft, ui.btnUp, ui.btnUpRight, ui.btnLeft,
                                ui.btnRight, ui.btnDownLeft, ui.btnDown, ui.btnDownRight};
    for (int i = 0; i < 8; ++i) {
        dirBtns[i]->setText("");
    }

    QToolButton* auxBtns[6] = {ui.btnZoomIn, ui.btnZoomOut, ui.btnFocusNear, ui.btnFocusFar, ui.btnIrisOpen, ui.btnIrisClose};
    for (int i = 0; i < 6; ++i) {
        auxBtns[i]->setText("");
    }

    ui.btnCenter->setText("");

    ui.presetListView->setModel(m_presetModel);
    ui.presetListView->hide();
    ui.btnPresetToggle->setArrowType(Qt::RightArrow);

    setupConnections();

    m_videoCellSubscription = EventBus::instance()->subscribe(TOPIC_VIDEO_CELL_SELECTED, [this](const QString& topic, EventDataPtr data) {
        onVideoCellSelected(topic, data);
    });
}

PTZControlPanel::~PTZControlPanel()
{
    if (!m_videoCellSubscription.isEmpty()) {
        EventBus::instance()->unsubscribe(m_videoCellSubscription);
    }
}

void PTZControlPanel::setupConnections()
{
    connect(ui.m_toggleBtn, &QToolButton::clicked, this, &PTZControlPanel::toggleCollapsed);
    connect(ui.btnPresetToggle, &QToolButton::clicked, this, &PTZControlPanel::togglePresetList);
    connect(ui.btnPresetAdd, &QToolButton::clicked, this, &PTZControlPanel::onAddPreset);
    connect(ui.btnPresetRemove, &QToolButton::clicked, this, &PTZControlPanel::onRemovePreset);
    connect(ui.presetListView, &QListView::clicked, this, &PTZControlPanel::onPresetClicked);
    connect(ui.btnCenter, &QToolButton::clicked, this, &PTZControlPanel::onCenterClicked);
    ui.btnCenter->setIcon(QIcon(":/icons/ic_ptz_center.svg"));
    ui.btnCenter->setIconSize(QSize(24, 24));
    ui.btnCenter->setToolTip(tr("Home"));

    m_directionMapper = new QSignalMapper(this);
    connect(m_directionMapper, QOverload<const QString&>::of(&QSignalMapper::mapped),
            this, &PTZControlPanel::onDirectionClicked);

    QString directions[8] = {"up_left", "up", "up_right", "left", "right", "down_left", "down", "down_right"};
    QString dirIcons[8] = {
        ":/icons/ic_ptz_up_left.svg",
        ":/icons/ic_ptz_up.svg",
        ":/icons/ic_ptz_up_right.svg",
        ":/icons/ic_ptz_left.svg",
        ":/icons/ic_ptz_right.svg",
        ":/icons/ic_ptz_down_left.svg",
        ":/icons/ic_ptz_down.svg",
        ":/icons/ic_ptz_down_right.svg"
    };
    QToolButton* dirBtns[8] = {ui.btnUpLeft, ui.btnUp, ui.btnUpRight, ui.btnLeft,
                                ui.btnRight, ui.btnDownLeft, ui.btnDown, ui.btnDownRight};

    for (int i = 0; i < 8; ++i) {
        dirBtns[i]->setIcon(QIcon(dirIcons[i]));
        dirBtns[i]->setIconSize(QSize(24, 24));
        dirBtns[i]->setCheckable(false);
        dirBtns[i]->setAutoRepeat(true);
        dirBtns[i]->setAutoRepeatDelay(200);
        dirBtns[i]->setAutoRepeatInterval(100);
        dirBtns[i]->setToolTip(directions[i]);
        // 方向由 QSignalMapper 携带，不在 lambda 里捕获
        connect(dirBtns[i], &QToolButton::clicked, m_directionMapper, QOverload<>::of(&QSignalMapper::map));
        m_directionMapper->setMapping(dirBtns[i], directions[i]);
    }

    m_auxMapper = new QSignalMapper(this);
    connect(m_auxMapper, QOverload<const QString&>::of(&QSignalMapper::mapped),
            this, &PTZControlPanel::onAuxFunctionClicked);

    QToolButton* auxBtns[6] = {ui.btnZoomIn, ui.btnZoomOut, ui.btnFocusNear, ui.btnFocusFar, ui.btnIrisOpen, ui.btnIrisClose};
    QString auxCmds[6] = {"zoom_in", "zoom_out", "focus_near", "focus_far", "iris_open", "iris_close"};
    QString auxIcons[6] = {
        ":/icons/ic_ptz_zoom_in.svg",
        ":/icons/ic_ptz_zoom_out.svg",
        ":/icons/ic_ptz_focus_near.svg",
        ":/icons/ic_ptz_focus_far.svg",
        ":/icons/ic_ptz_iris_open.svg",
        ":/icons/ic_ptz_iris_close.svg"
    };
    QString auxTooltips[6] = {"Zoom In", "Zoom Out", "Focus Near", "Focus Far", "Iris Open", "Iris Close"};

    for (int i = 0; i < 6; ++i) {
        auxBtns[i]->setText("");
        auxBtns[i]->setIcon(QIcon(auxIcons[i]));
        auxBtns[i]->setIconSize(QSize(24, 24));
        auxBtns[i]->setAutoRepeat(true);
        auxBtns[i]->setAutoRepeatDelay(200);
        auxBtns[i]->setAutoRepeatInterval(100);
        auxBtns[i]->setToolTip(auxTooltips[i]);
        // 辅助命令由 QSignalMapper 携带，不在 lambda 里捕获
        connect(auxBtns[i], &QToolButton::clicked, m_auxMapper, QOverload<>::of(&QSignalMapper::map));
        m_auxMapper->setMapping(auxBtns[i], auxCmds[i]);
    }
}

void PTZControlPanel::toggleCollapsed()
{
    m_collapsed = !m_collapsed;

    if (m_collapsed) {
        ui.m_contentWidget->hide();
        ui.m_toggleBtn->setArrowType(Qt::RightArrow);
        ui.m_toggleBtn->setToolTip(tr("Expand"));
        setFixedHeight(ui.m_titleBar->height());
    } else {
        ui.m_contentWidget->show();
        ui.m_toggleBtn->setArrowType(Qt::DownArrow);
        ui.m_toggleBtn->setToolTip(tr("Collapse"));
        setMinimumHeight(0);
        setMaximumHeight(16777215);
    }
}

void PTZControlPanel::togglePresetList()
{
    m_presetListCollapsed = !m_presetListCollapsed;

    if (m_presetListCollapsed) {
        ui.presetListView->hide();
        ui.btnPresetToggle->setArrowType(Qt::RightArrow);
    } else {
        ui.presetListView->show();
        ui.btnPresetToggle->setArrowType(Qt::DownArrow);
    }
}

void PTZControlPanel::setEnabled(bool enabled)
{
    m_enabled = enabled;

    QToolButton* dirBtns[8] = {ui.btnUpLeft, ui.btnUp, ui.btnUpRight, ui.btnLeft,
                                ui.btnRight, ui.btnDownLeft, ui.btnDown, ui.btnDownRight};
    for (int i = 0; i < 8; ++i) {
        dirBtns[i]->setEnabled(enabled);
    }

    ui.btnCenter->setEnabled(enabled);

    QToolButton* auxBtns[6] = {ui.btnZoomIn, ui.btnZoomOut, ui.btnFocusNear, ui.btnFocusFar, ui.btnIrisOpen, ui.btnIrisClose};
    for (int i = 0; i < 6; ++i) {
        auxBtns[i]->setEnabled(enabled);
    }

    ui.btnPresetAdd->setEnabled(enabled);
    ui.btnPresetRemove->setEnabled(enabled);
    ui.presetListView->setEnabled(enabled);
}

void PTZControlPanel::onDeviceSelected(const QString& deviceId, const QString& channelId)
{
    m_deviceId = deviceId;
    m_channelId = channelId;
    LOG_INFO("PTZControlPanel", QString("Device selected: %1, Channel: %2").arg(deviceId).arg(channelId));
}

void PTZControlPanel::onDirectionClicked(const QString& direction)
{
    emit ptzCommand("direction", direction);
}

void PTZControlPanel::onCenterClicked()
{
    emit ptzCommand("center", true);
    LOG_INFO("PTZControlPanel", "Center/Home clicked");
}

void PTZControlPanel::onPresetClicked(const QModelIndex& index)
{
    QString presetName = index.data().toString();
    emit ptzCommand("preset", presetName);
    LOG_INFO("PTZControlPanel", QString("Preset clicked: %1").arg(presetName));
}

void PTZControlPanel::onAuxFunctionClicked(const QString& function)
{
    emit ptzCommand("aux", function);
}

void PTZControlPanel::onAddPreset()
{
    emit ptzCommand("add_preset", QString("New Preset"));
}

void PTZControlPanel::onRemovePreset()
{
    QModelIndex currentIndex = ui.presetListView->currentIndex();
    if (currentIndex.isValid()) {
        QString presetName = currentIndex.data().toString();
        emit ptzCommand("delete_preset", presetName);
    }
}

void PTZControlPanel::loadPresets(const QString& itemId)
{
    QStringList parts = itemId.split("/");
    if (parts.size() != 2) {
        LOG_ERROR("PTZControlPanel", QString("Invalid itemId format: %1").arg(itemId));
        return;
    }
    QString deviceTag = parts[0];
    QString channelTag = parts[1];

    QString apiPath = QString("/api/v1/channels/%1/%2/ptz/presets").arg(deviceTag, channelTag);
    ServerApi::instance()->get(apiPath, [this](bool success, const QJsonObject& resp, const QString& error) {
        if (!success) {
            LOG_ERROR("PTZControlPanel", QString("Failed to load presets: %1").arg(error));
            return;
        }
        QStringList presets;
        if (resp.contains("presets")) {
            QJsonArray presetsArray = resp["presets"].toArray();
            for (const QJsonValue& val : presetsArray) {
                presets.append(val.toString());
            }
        }
        QMetaObject::invokeMethod(this, [this, presets]() {
            m_presetModel->setStringList(presets);
            LOG_INFO("PTZControlPanel", QString("Loaded %1 presets").arg(presets.size()));
        }, Qt::QueuedConnection);
    });
}

void PTZControlPanel::onVideoCellSelected(const QString& topic, EventDataPtr data)
{
    Q_UNUSED(topic)
    if (data && data->m_payload.contains("itemId")) {
        QString itemId = data->m_payload["itemId"].toString();
        LOG_INFO("PTZControlPanel", QString("Video cell selected: %1").arg(itemId));
        QMetaObject::invokeMethod(this, [this, itemId]() {
            loadPresets(itemId);
        }, Qt::QueuedConnection);
    }
}

}
