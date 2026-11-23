#include "MapLoadingOverlay.h"

#include <QEvent>
#include <QLabel>
#include <QProgressBar>
#include <QPushButton>
#include <QVBoxLayout>

namespace rustcam {

MapLoadingOverlay::MapLoadingOverlay(QWidget* parent)
    : QWidget(parent)
{
    setObjectName(QStringLiteral("mapLoadingOverlay"));
    // 必须是原生窗口：远端 CEF 子窗口是原生窗口，普通 Qt 兄弟控件会被它遮挡。
    setAttribute(Qt::WA_NativeWindow);
    setAttribute(Qt::WA_StyledBackground, true);

    setStyleSheet(QStringLiteral(
        "QWidget#mapLoadingOverlay { background-color: #1a1a1a; }"
        "QLabel#mapLoadingText { color: #c8c8c8; font-size: 14px; }"
        "QProgressBar#mapLoadingBar { background-color: #2b2b2b; border: none;"
        " border-radius: 3px; }"
        "QProgressBar#mapLoadingBar::chunk { background-color: #0078d4;"
        " border-radius: 3px; }"
        "QPushButton#mapLoadingRetry { min-width: 96px; min-height: 32px;"
        " color: #e0e0e0; background-color: #2d2d30; border: 1px solid #3e3e3e;"
        " border-radius: 4px; }"
        "QPushButton#mapLoadingRetry:hover { border-color: #0078d4; }"));

    QVBoxLayout* layout = new QVBoxLayout(this);
    layout->setContentsMargins(24, 24, 24, 24);
    layout->setSpacing(12);
    layout->addStretch();

    m_text = new QLabel(this);
    m_text->setObjectName(QStringLiteral("mapLoadingText"));
    m_text->setAlignment(Qt::AlignCenter);
    layout->addWidget(m_text);

    m_progress = new QProgressBar(this);
    m_progress->setObjectName(QStringLiteral("mapLoadingBar"));
    m_progress->setRange(0, 0);
    m_progress->setTextVisible(false);
    m_progress->setFixedSize(220, 6);
    layout->addWidget(m_progress, 0, Qt::AlignHCenter);

    m_retry = new QPushButton(tr("重试"), this);
    m_retry->setObjectName(QStringLiteral("mapLoadingRetry"));
    m_retry->setCursor(Qt::PointingHandCursor);
    m_retry->hide();
    layout->addWidget(m_retry, 0, Qt::AlignHCenter);

    layout->addStretch();

    connect(m_retry, &QPushButton::clicked, this,
            &MapLoadingOverlay::retryRequested);

    if (parent != nullptr) {
        parent->installEventFilter(this);
        syncGeometry();
    }
}

void MapLoadingOverlay::showBusy(const QString& text)
{
    m_text->setText(text);
    m_progress->show();
    m_retry->hide();
    show();
    raise();
}

void MapLoadingOverlay::showError(const QString& text)
{
    m_text->setText(text.isEmpty() ? tr("地图加载失败")
                                   : tr("地图加载失败：%1").arg(text));
    m_progress->hide();
    m_retry->show();
    show();
    raise();
}

bool MapLoadingOverlay::eventFilter(QObject* watched, QEvent* event)
{
    if (watched == parentWidget() && event->type() == QEvent::Resize) {
        syncGeometry();
    }
    return QWidget::eventFilter(watched, event);
}

void MapLoadingOverlay::showEvent(QShowEvent* event)
{
    QWidget::showEvent(event);
    syncGeometry();
    // CEF 的 OnAfterCreated 会把远端窗口提到最前，遮罩必须重新压在它上面。
    raise();
}

void MapLoadingOverlay::syncGeometry()
{
    if (QWidget* parent = parentWidget()) {
        setGeometry(parent->rect());
    }
}

} // namespace rustcam
