#include "TourChannelModel.h"

#include <QBrush>
#include <QColor>

namespace rustcam {

TourChannelModel::TourChannelModel(QObject* parent)
    : QAbstractTableModel(parent)
{
}

void TourChannelModel::reset(const QVector<TourChannel>& channels)
{
    beginResetModel();
    m_rows.clear();
    m_rows.reserve(channels.size());
    for (const TourChannel& channel : channels) {
        Row row;
        row.channel = channel;
        row.state = TourChannelState::Pending;
        m_rows.append(row);
    }
    endResetModel();
}

void TourChannelModel::setState(const TourChannelRef& channel, TourChannelState state,
                                const QString& reason, int batchIndex)
{
    for (int i = 0; i < m_rows.size(); ++i) {
        if (m_rows.at(i).channel.ref() != channel) {
            continue;
        }
        m_rows[i].state = state;
        if (!reason.isEmpty()) {
            m_rows[i].reason = reason;
        }
        if (batchIndex >= 0) {
            m_rows[i].batch = batchIndex;
        }
        const QModelIndex changed = index(i, 0);
        emit dataChanged(changed, index(i, ColumnCount - 1));
        return;
    }
}

void TourChannelModel::setCurrentBatch(int batchIndex)
{
    if (m_currentBatch == batchIndex) {
        return;
    }
    m_currentBatch = batchIndex;
    if (m_rows.isEmpty()) {
        return;
    }
    emit dataChanged(index(0, 0), index(m_rows.size() - 1, ColumnCount - 1));
}

int TourChannelModel::rowCount(const QModelIndex& parent) const
{
    return parent.isValid() ? 0 : m_rows.size();
}

int TourChannelModel::columnCount(const QModelIndex& parent) const
{
    return parent.isValid() ? 0 : ColumnCount;
}

QVariant TourChannelModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= m_rows.size()) {
        return QVariant();
    }
    const Row& row = m_rows.at(index.row());

    switch (role) {
    case Qt::DisplayRole:
        switch (index.column()) {
        case NameColumn:
            return row.channel.name.isEmpty()
                       ? row.channel.deviceTag + QLatin1Char('/') + row.channel.channelTag
                       : row.channel.name;
        case StateColumn:
            return tourChannelStateText(row.state);
        case ReasonColumn:
            return row.reason;
        case BatchColumn:
            return row.batch >= 0 ? QVariant(row.batch + 1) : QVariant();
        default:
            return QVariant();
        }
    case Qt::BackgroundRole:
        if (m_currentBatch >= 0 && row.batch == m_currentBatch) {
            return QBrush(QColor(QStringLiteral("#1f3a5f")));
        }
        return QVariant();
    case Qt::ToolTipRole:
        return row.channel.deviceTag + QLatin1Char('/') + row.channel.channelTag;
    default:
        return QVariant();
    }
}

QVariant TourChannelModel::headerData(int section, Qt::Orientation orientation, int role) const
{
    if (orientation != Qt::Horizontal || role != Qt::DisplayRole) {
        return QVariant();
    }
    switch (section) {
    case NameColumn:   return QStringLiteral("通道名");
    case StateColumn:  return QStringLiteral("状态");
    case ReasonColumn: return QStringLiteral("失败原因");
    case BatchColumn:  return QStringLiteral("所在批");
    default:           return QVariant();
    }
}

} // namespace rustcam
