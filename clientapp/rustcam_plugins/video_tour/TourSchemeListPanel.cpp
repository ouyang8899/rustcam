#include "TourSchemeListPanel.h"

#include <QListWidget>
#include <QMessageBox>
#include <QPushButton>
#include <QVBoxLayout>

#include "TourApi.h"
#include "TourAuditLog.h"
#include "TourSchemeEditorDialog.h"

namespace rustcam {

TourSchemeListPanel::TourSchemeListPanel(TourApi* api, QWidget* parent)
    : QWidget(parent)
    , m_api(api)
{
    QVBoxLayout* layout = new QVBoxLayout(this);
    layout->setContentsMargins(4, 4, 4, 4);
    layout->setSpacing(4);

    m_list = new QListWidget(this);
    m_list->setSelectionMode(QAbstractItemView::SingleSelection);
    layout->addWidget(m_list, 1);

    QHBoxLayout* buttons = new QHBoxLayout();
    m_newButton = new QPushButton(tr("新建"), this);
    m_editButton = new QPushButton(tr("编辑"), this);
    m_deleteButton = new QPushButton(tr("删除"), this);
    m_startButton = new QPushButton(tr("开始"), this);
    buttons->addWidget(m_newButton);
    buttons->addWidget(m_editButton);
    buttons->addWidget(m_deleteButton);
    buttons->addWidget(m_startButton);
    layout->addLayout(buttons);

    connect(m_list, &QListWidget::itemActivated, this, &TourSchemeListPanel::onItemActivated);
    connect(m_list, &QListWidget::itemSelectionChanged, this, &TourSchemeListPanel::onSelectionChanged);
    connect(m_newButton, &QPushButton::clicked, this, &TourSchemeListPanel::onNewScheme);
    connect(m_editButton, &QPushButton::clicked, this, &TourSchemeListPanel::onEditScheme);
    connect(m_deleteButton, &QPushButton::clicked, this, &TourSchemeListPanel::onDeleteScheme);
    connect(m_startButton, &QPushButton::clicked, this, &TourSchemeListPanel::onStartScheme);

    onSelectionChanged();
}

void TourSchemeListPanel::refresh()
{
    if (!m_api) {
        return;
    }
    m_api->listSchemes([this](bool success, const QVector<TourScheme>& schemes, const QString& error) {
        if (!success) {
            emit notify(tr("加载方案列表失败：%1").arg(error), true);
            return;
        }
        m_schemes = schemes;
        m_list->clear();
        for (const TourScheme& scheme : m_schemes) {
            const QString text = QStringLiteral("%1  [锁定%2路]")
                                     .arg(scheme.name)
                                     .arg(scheme.lockedSet.size());
            QListWidgetItem* item = new QListWidgetItem(text, m_list);
            item->setData(Qt::UserRole, scheme.id);
        }
        emit schemesChanged();
        onSelectionChanged();
    });
}

TourScheme* TourSchemeListPanel::selectedScheme() const
{
    QListWidgetItem* item = m_list->currentItem();
    if (!item) {
        return nullptr;
    }
    const qint64 id = item->data(Qt::UserRole).toLongLong();
    for (const TourScheme& scheme : m_schemes) {
        if (scheme.id == id) {
            return const_cast<TourScheme*>(&scheme);
        }
    }
    return nullptr;
}

bool TourSchemeListPanel::currentScheme(TourScheme* out) const
{
    TourScheme* scheme = selectedScheme();
    if (!scheme || !out) {
        return false;
    }
    *out = *scheme;
    return true;
}

void TourSchemeListPanel::onSelectionChanged()
{
    const bool hasSelection = m_list->currentItem() != nullptr;
    m_editButton->setEnabled(hasSelection);
    m_deleteButton->setEnabled(hasSelection);
    m_startButton->setEnabled(hasSelection);
}

void TourSchemeListPanel::onItemActivated(QListWidgetItem* item)
{
    Q_UNUSED(item)
    onEditScheme();
}

void TourSchemeListPanel::onNewScheme()
{
    if (!m_api) {
        return;
    }
    TourScheme blank;
    blank.layoutId = m_layouts.isEmpty() ? 0 : m_layouts.first().id;
    blank.dwellMs = 10000;
    blank.batchTimeoutMs = 5000;
    blank.switchGapMs = 1000;
    blank.cycleGapMs = 0;
    blank.backfillLimit = 2;
    blank.maxCycles = 0;
    blank.safetyMaxDurationMs = 3600000;

    TourSchemeEditorDialog dialog(m_api, m_layouts, this);
    dialog.editScheme(blank);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    const TourScheme scheme = dialog.scheme();
    m_api->createScheme(scheme, [this](bool success, const TourScheme& created, const QString& error) {
        if (!success) {
            emit notify(tr("新建方案失败：%1").arg(error), true);
            return;
        }
        TourAuditLog::log(QStringLiteral("scheme_created"));
        refresh();
    });
}

void TourSchemeListPanel::onEditScheme()
{
    TourScheme scheme;
    if (!currentScheme(&scheme) || !m_api) {
        return;
    }
    TourSchemeEditorDialog dialog(m_api, m_layouts, this);
    dialog.editScheme(scheme);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    const TourScheme edited = dialog.scheme();
    const int expectedVersion = scheme.version;
    m_api->updateScheme(edited, expectedVersion, [this](bool success, const QString& error) {
        if (!success) {
            emit notify(tr("保存方案失败：%1").arg(error), true);
            refresh();
            return;
        }
        TourAuditLog::log(QStringLiteral("scheme_updated"));
        refresh();
    });
}

void TourSchemeListPanel::onDeleteScheme()
{
    TourScheme scheme;
    if (!currentScheme(&scheme) || !m_api) {
        return;
    }
    const auto answer = QMessageBox::question(this, tr("删除方案"),
                                               tr("确定删除方案“%1”吗？").arg(scheme.name),
                                               QMessageBox::Yes | QMessageBox::No);
    if (answer != QMessageBox::Yes) {
        return;
    }
    m_api->deleteScheme(scheme.id, [this](bool success, const QString& error) {
        if (!success) {
            emit notify(tr("删除方案失败：%1").arg(error), true);
            return;
        }
        TourAuditLog::log(QStringLiteral("scheme_deleted"));
        refresh();
    });
}

void TourSchemeListPanel::onStartScheme()
{
    TourScheme scheme;
    if (!currentScheme(&scheme)) {
        return;
    }
    // 全锁校验需要布局窗口数，由运行页在启动前给出。
    emit startRequested(scheme);
}

} // namespace rustcam
