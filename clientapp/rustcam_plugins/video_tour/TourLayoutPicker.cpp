#include "TourLayoutPicker.h"

#include "TourTypes.h"
#include "core/ThemeManager.h"

#include <QGridLayout>
#include <QLabel>
#include <QToolButton>
#include <QVBoxLayout>

namespace rustcam {

struct TourLayoutPicker::CellWidgets {
    QToolButton* button = nullptr;
    QLabel* nameLabel = nullptr;
    QLabel* channelLabel = nullptr;
};

TourLayoutPicker::TourLayoutPicker(QWidget* parent)
    : QWidget(parent)
    , m_grid(new QGridLayout(this))
{
    m_grid->setContentsMargins(0, 0, 0, 0);
    m_grid->setSpacing(6);
    setMinimumSize(320, 220);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);

    // 主题切换后重新上色，让缩略图跟着主题走。
    connect(ThemeManager::instance(), &ThemeManager::themeChanged, this,
            [this](ThemeManager::Theme) { refreshCells(); });
}

void TourLayoutPicker::setLayout(const PlayerLayout& layout)
{
    m_layout = layout;
    rebuildCells();
}

void TourLayoutPicker::setLocks(const QVector<LockedSlot>& locks,
                                 const QHash<QString, QString>& channelNames)
{
    m_locks = locks;
    m_channelNames = channelNames;
    refreshCells();
}

void TourLayoutPicker::selectItem(const QString& layoutItemId)
{
    m_selectedItemId = layoutItemId;
    for (auto it = m_cells.begin(); it != m_cells.end(); ++it) {
        it.value()->button->setChecked(it.key() == layoutItemId);
    }
}

void TourLayoutPicker::rebuildCells()
{
    for (auto it = m_cells.begin(); it != m_cells.end(); ++it) {
        // 删除按钮会连带删除它内部的标签。
        delete it.value()->button;
    }
    m_cells.clear();
    m_selectedItemId.clear();

    for (const LayoutItem& item : m_layout.layoutJson) {
        QToolButton* button = new QToolButton(this);
        button->setCheckable(true);
        button->setAutoExclusive(true);
        // QToolButton 的 sizeHint 只按文字/图标算，格子里放的是标签，
        // 这里给一个下限，避免布局较密时文字被裁掉。
        button->setMinimumSize(96, 64);
        button->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
        button->setCursor(Qt::PointingHandCursor);
        button->setToolTip(layoutItemDisplayName(item));
        button->setProperty("layoutItemId", item.id);

        QVBoxLayout* inner = new QVBoxLayout(button);
        inner->setContentsMargins(4, 4, 4, 4);
        inner->setSpacing(2);

        QLabel* nameLabel = new QLabel(layoutItemDisplayName(item), button);
        nameLabel->setObjectName(QStringLiteral("tourCellName"));
        nameLabel->setAlignment(Qt::AlignCenter);
        nameLabel->setAttribute(Qt::WA_TransparentForMouseEvents);
        inner->addWidget(nameLabel);

        QLabel* channelLabel = new QLabel(button);
        channelLabel->setObjectName(QStringLiteral("tourCellChannel"));
        channelLabel->setAlignment(Qt::AlignCenter);
        channelLabel->setAttribute(Qt::WA_TransparentForMouseEvents);
        inner->addWidget(channelLabel);

        m_grid->addWidget(button, item.row, item.col,
                          qMax(1, item.rowSpan), qMax(1, item.colSpan));

        CellWidgets* cell = new CellWidgets;
        cell->button = button;
        cell->nameLabel = nameLabel;
        cell->channelLabel = channelLabel;
        m_cells.insert(item.id, cell);

        const QString itemId = item.id;
        connect(button, &QToolButton::clicked, this, [this, itemId]() {
            m_selectedItemId = itemId;
            emit itemSelected(itemId);
        });
    }

    refreshCells();
}

void TourLayoutPicker::refreshCells()
{
    ThemeManager* theme = ThemeManager::instance();
    const QColor border = theme->getColor("border", QColor("#2a2a2a"));
    const QColor accent = theme->getColor("accent", QColor("#0078D4"));
    const QColor warning = theme->getColor("warning", QColor("#ff9800"));
    const QColor textPrimary = theme->getColor("text_primary", QColor("#e0e0e0"));

    QHash<QString, QString> lockedChannels;
    for (const LockedSlot& slot : m_locks) {
        lockedChannels.insert(slot.layoutItemId, m_channelNames.value(slot.ref().key()));
    }

    for (auto it = m_cells.begin(); it != m_cells.end(); ++it) {
        CellWidgets* cell = it.value();
        const bool locked = lockedChannels.contains(it.key());
        const QString channelName = lockedChannels.value(it.key());

        cell->nameLabel->setStyleSheet(
            QStringLiteral("color: %1; font-weight: bold;").arg(textPrimary.name()));
        cell->channelLabel->setText(locked ? channelName : QString());
        cell->channelLabel->setStyleSheet(
            QStringLiteral("color: %1;").arg(warning.name()));
        cell->button->setToolTip(
            locked ? QStringLiteral("%1\n%2").arg(cell->nameLabel->text(), channelName)
                   : cell->nameLabel->text());

        // 局部展示窗口用告警色描边并加浅底色，普通窗口保持中性描边。
        const QString normalStyle =
            QStringLiteral("QToolButton { border: 1px solid %1; border-radius: 4px;"
                           " background-color: transparent; }"
                           "QToolButton:checked { border: 2px solid %2; }")
                .arg(border.name(), accent.name());
        const QString lockedStyle =
            QStringLiteral("QToolButton { border: 2px solid %1; border-radius: 4px;"
                           " background-color: rgba(%2, %3, %4, 40); }"
                           "QToolButton:checked { border: 2px solid %5; }")
                .arg(warning.name())
                .arg(warning.red()).arg(warning.green()).arg(warning.blue())
                .arg(accent.name());

        cell->button->setStyleSheet(locked ? lockedStyle : normalStyle);
    }
}

} // namespace rustcam
