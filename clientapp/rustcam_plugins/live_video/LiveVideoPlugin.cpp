#include "LiveVideoPlugin.h"
#include "core/EventBus.h"
#include "core/Logger.h"
#include <QWidget>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QGridLayout>
#include <QStackedWidget>
#include <QToolBar>
#include <QAction>
#include <QLabel>
#include <QComboBox>

namespace rustcam {

class VideoCell : public QWidget {
    Q_OBJECT
public:
    VideoCell(const QString& cellId, QWidget* parent = nullptr)
        : QWidget(parent)
        , m_cellId(cellId)
        , m_playing(false)
    {
        setupUi();
    }

    QString cellId() const { return m_cellId; }
    bool isPlaying() const { return m_playing; }

    void setTitle(const QString& title) {
        m_titleLabel->setText(title);
    }

    void setPlaying(bool playing) {
        m_playing = playing;
        if (playing) {
            m_overlayWidget->hide();
        } else {
            m_overlayWidget->show();
        }
    }

    void updateOsd(const QString& info) {
        m_osdLabel->setText(info);
    }

private slots:
    void onCellClicked() {
        emit cellSelected(m_cellId);
    }

signals:
    void cellSelected(const QString& cellId);

private:
    void setupUi() {
        setStyleSheet(R"(
            VideoCell {
                background-color: #000000;
                border: 1px solid rgba(255, 255, 255, 0.08);
                border-radius: 4px;
            }
            VideoCell:hover {
                border: 1px solid #0078D4;
            }
        )");

        QVBoxLayout* layout = new QVBoxLayout(this);
        layout->setContentsMargins(0, 0, 0, 0);

        m_videoLabel = new QLabel(this);
        m_videoLabel->setAlignment(Qt::AlignCenter);
        m_videoLabel->setText("NO SIGNAL");
        m_videoLabel->setStyleSheet(R"(
            QLabel {
                color: #666;
                font-size: 14px;
                background-color: #000;
            }
        )");
        layout->addWidget(m_videoLabel);

        m_overlayWidget = new QWidget(this);
        m_overlayWidget->setStyleSheet("background-color: rgba(0, 0, 0, 0.7);");
        QVBoxLayout* overlayLayout = new QVBoxLayout(m_overlayWidget);
        overlayLayout->setAlignment(Qt::AlignCenter);
        QLabel* iconLabel = new QLabel("📹", m_overlayWidget);
        iconLabel->setAlignment(Qt::AlignCenter);
        iconLabel->setStyleSheet("font-size: 32px;");
        overlayLayout->addWidget(iconLabel);
        QLabel* textLabel = new QLabel("离线", m_overlayWidget);
        textLabel->setAlignment(Qt::AlignCenter);
        textLabel->setStyleSheet("color: #666; font-size: 12px;");
        overlayLayout->addWidget(textLabel);

        m_osdLabel = new QLabel(this);
        m_osdLabel->setAlignment(Qt::AlignLeft | Qt::AlignTop);
        m_osdLabel->setStyleSheet(R"(
            QLabel {
                color: white;
                background-color: rgba(0, 0, 0, 0.6);
                padding: 4px 8px;
                border-radius: 2px;
                font-size: 11px;
            }
        )");
        m_osdLabel->hide();

        m_titleLabel = new QLabel(this);
        m_titleLabel->setAlignment(Qt::AlignLeft | Qt::AlignBottom);
        m_titleLabel->setStyleSheet(R"(
            QLabel {
                color: white;
                background-color: rgba(0, 0, 0, 0.6);
                padding: 4px 8px;
                border-radius: 2px;
                font-size: 12px;
                font-weight: bold;
            }
        )");
        m_titleLabel->hide();

        setMinimumSize(320, 180);
    }

    QString m_cellId;
    bool m_playing;
    QLabel* m_videoLabel;
    QWidget* m_overlayWidget;
    QLabel* m_osdLabel;
    QLabel* m_titleLabel;
};

bool LiveVideoPlugin::initialize(EventBus* bus, QObject* cfg)
{
    Q_UNUSED(cfg)
    m_currentLayout = 4;
    m_maxCells = 16;

    EventBus::instance()->subscribe("video/stream/request", this, SLOT(onStreamRequested(QString,QVariantMap)));
    EventBus::instance()->subscribe("video/stream/stop", this, SLOT(onStreamStop(QString,QVariantMap)));

    LOG_INFO("LiveVideoPlugin", "Initialized");
    return true;
}

void LiveVideoPlugin::shutdown()
{
    EventBus::instance()->unsubscribeAll(this);
}

QWidget* LiveVideoPlugin::createWorkspace(QWidget* parent)
{
    m_workspace = new QWidget(parent);
    m_workspace->setObjectName("workspaceArea");
    m_workspace->setStyleSheet(R"(
        QWidget#workspaceArea {
            background-color: #2b2b2b;
        }
    )");

    QVBoxLayout* layout = new QVBoxLayout(m_workspace);
    layout->setContentsMargins(8, 8, 8, 8);
    layout->setSpacing(8);

    QToolBar* toolbar = new QToolBar(m_workspace);
    toolbar->setStyleSheet(R"(
        QToolBar {
            background-color: #1e1e1e;
            border: none;
            spacing: 8px;
            padding: 4px;
        }
    )");

    QLabel* layoutLabel = new QLabel("布局:");
    layoutLabel->setStyleSheet("color: #e0e0e0;");
    toolbar->addWidget(layoutLabel);

    QComboBox* layoutCombo = new QComboBox();
    layoutCombo->addItems({"1", "4", "6", "8", "9", "13", "16"});
    layoutCombo->setCurrentText("4");
    layoutCombo->setStyleSheet(R"(
        QComboBox {
            background-color: #2b2b2b;
            border: 1px solid #3e3e3e;
            border-radius: 4px;
            padding: 4px 8px;
            color: #e0e0e0;
        }
    )");
    connect(layoutCombo, &QComboBox::currentTextChanged, this, [this](const QString& text) {
        setLayout(text.toInt());
    });
    toolbar->addWidget(layoutCombo);

    toolbar->addSeparator();

    QAction* stopAllAction = new QAction("停止全部", this);
    stopAllAction->setIconText("⏹");
    connect(stopAllAction, &QAction::triggered, this, [this]() {
        for (auto cell : m_cells.values()) {
            stopStream(cell->cellId());
        }
    });
    toolbar->addAction(stopAllAction);

    layout->addWidget(toolbar);

    QWidget* gridContainer = new QWidget(m_workspace);
    QGridLayout* gridLayout = new QGridLayout(gridContainer);
    gridLayout->setContentsMargins(0, 0, 0, 0);
    gridLayout->setSpacing(8);

    for (int i = 0; i < 16; ++i) {
        QString cellId = QString("cell-%1").arg(i);
        VideoCell* cell = new VideoCell(cellId, gridContainer);
        m_cells[cellId] = cell;
        cell->hide();
    }

    updateLayout();
    layout->addWidget(gridContainer, 1);

    return m_workspace;
}

void LiveVideoPlugin::updateLayout()
{
    static QMap<int, QList<QPair<int,int>>> layouts = {
        {1, {{0,0}}},
        {4, {{0,0},{0,1},{1,0},{1,1}}},
        {6, {{0,0},{0,1},{0,2},{1,0},{1,1},{1,2}}},
        {8, {{0,0},{0,1},{0,2},{0,3},{1,0},{1,1},{1,2},{1,3}}},
        {9, {{0,0},{0,1},{0,2},{1,0},{1,1},{1,2},{2,0},{2,1},{2,2}}},
        {16, {{0,0},{0,1},{0,2},{0,3},{1,0},{1,1},{1,2},{1,3},{2,0},{2,1},{2,2},{2,3},{3,0},{3,1},{3,2},{3,3}}}
    };

    QList<QPair<int,int>> positions = layouts.value(m_currentLayout);

    QStringList activeCells;
    for (const auto& pos : positions) {
        int row = pos.first;
        int col = pos.second;
        int index = row * 4 + col;
        QString cellId = QString("cell-%1").arg(index);
        activeCells.append(cellId);
    }

    QGridLayout* gridLayout = qobject_cast<QGridLayout*>(m_cells["cell-0"]->parentWidget()->layout());

    for (auto it = m_cells.begin(); it != m_cells.end(); ++it) {
        QString cellId = it.key();
        VideoCell* cell = it.value();

        if (activeCells.contains(cellId)) {
            cell->show();
            int index = activeCells.indexOf(cellId);
            int row = index / 4;
            int col = index % 4;
            gridLayout->addWidget(cell, row, col);
        } else {
            cell->hide();
        }
    }
}

VideoCell* LiveVideoPlugin::getCell(const QString& cellId)
{
    return m_cells.value(cellId);
}

void LiveVideoPlugin::setLayout(int cells)
{
    m_currentLayout = cells;
    updateLayout();
}

void LiveVideoPlugin::playStream(const QString& cellId, const QString& url, const QString& title)
{
    VideoCell* cell = getCell(cellId);
    if (cell) {
        cell->setTitle(title);
        cell->setPlaying(true);
        LOG_INFO("LiveVideoPlugin", QString("Playing: %1 -> %2").arg(cellId).arg(url));
        emit streamStarted(cellId);
    }
}

void LiveVideoPlugin::stopStream(const QString& cellId)
{
    VideoCell* cell = getCell(cellId);
    if (cell) {
        cell->setPlaying(false);
        LOG_INFO("LiveVideoPlugin", QString("Stopped: %1").arg(cellId));
        emit streamStopped(cellId);
    }
}

void LiveVideoPlugin::onStreamRequested(const QString& topic, const QVariantMap& payload)
{
    Q_UNUSED(topic)
    QString cellId = payload["cellId"].toString();
    QString url = payload["url"].toString();
    QString title = payload["title"].toString();

    if (cellId.isEmpty()) {
        for (auto cell : m_cells.values()) {
            if (!cell->isPlaying()) {
                cellId = cell->cellId();
                break;
            }
        }
    }

    playStream(cellId, url, title);
}

void LiveVideoPlugin::onStreamStop(const QString& topic, const QVariantMap& payload)
{
    Q_UNUSED(topic)
    QString cellId = payload["cellId"].toString();
    if (!cellId.isEmpty()) {
        stopStream(cellId);
    }
}

}

#include "LiveVideoPlugin.moc"
