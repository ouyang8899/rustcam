#include "TourChannelStatusPanel.h"

#include <QHeaderView>
#include <QHBoxLayout>
#include <QLabel>
#include <QTableView>
#include <QVBoxLayout>

#include "TourChannelModel.h"

namespace rustcam {

TourChannelStatusPanel::TourChannelStatusPanel(QWidget* parent)
    : QWidget(parent)
    , m_model(new TourChannelModel(this))
{
    QVBoxLayout* layout = new QVBoxLayout(this);
    layout->setContentsMargins(4, 4, 4, 4);
    layout->setSpacing(4);

    QHBoxLayout* header = new QHBoxLayout();
    header->setContentsMargins(0, 0, 0, 0);
    m_titleLabel = new QLabel(tr("轮询通道状态"), this);
    header->addWidget(m_titleLabel);
    header->addStretch(1);
    layout->addLayout(header);

    m_tableView = new QTableView(this);
    m_tableView->setModel(m_model);
    m_tableView->setMinimumHeight(120);
    m_tableView->setSelectionBehavior(QAbstractItemView::SelectRows);
    m_tableView->setEditTriggers(QAbstractItemView::NoEditTriggers);
    QHeaderView* viewHeader = m_tableView->horizontalHeader();
    viewHeader->setStretchLastSection(false);
    viewHeader->setSectionResizeMode(TourChannelModel::NameColumn, QHeaderView::Stretch);
    viewHeader->setSectionResizeMode(TourChannelModel::StateColumn, QHeaderView::ResizeToContents);
    viewHeader->setSectionResizeMode(TourChannelModel::ReasonColumn, QHeaderView::Stretch);
    viewHeader->setSectionResizeMode(TourChannelModel::BatchColumn, QHeaderView::ResizeToContents);
    layout->addWidget(m_tableView, 1);
}

int TourChannelStatusPanel::selectedRow() const
{
    if (!m_tableView || !m_tableView->selectionModel()) {
        return -1;
    }
    const QModelIndexList selection = m_tableView->selectionModel()->selectedRows();
    if (selection.isEmpty()) {
        return -1;
    }
    return selection.first().row();
}

} // namespace rustcam
