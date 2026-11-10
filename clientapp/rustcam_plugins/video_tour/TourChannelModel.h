#pragma once

#include "TourScheme.h"
#include "TourTypes.h"

#include <QAbstractTableModel>
#include <QVector>

namespace rustcam {

/// 通道状态表：通道名 / 状态 / 失败原因 / 所在批。
class TourChannelModel : public QAbstractTableModel {
    Q_OBJECT
public:
    enum Column {
        NameColumn = 0,
        StateColumn,
        ReasonColumn,
        BatchColumn,
        ColumnCount
    };

    explicit TourChannelModel(QObject* parent = nullptr);

    /// 重置为方案里的全部通道。
    void reset(const QVector<TourChannel>& channels);
    /// 更新某通道的状态；batchIndex < 0 表示不改动所在批。
    void setState(const TourChannelRef& channel, TourChannelState state,
                  const QString& reason = QString(), int batchIndex = -1);
    /// 标记当前批次（用于高亮）。
    void setCurrentBatch(int batchIndex);

    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    int columnCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    QVariant headerData(int section, Qt::Orientation orientation,
                        int role = Qt::DisplayRole) const override;

private:
    struct Row {
        TourChannel channel;
        TourChannelState state = TourChannelState::Pending;
        QString reason;
        int batch = -1;
    };

    QVector<Row> m_rows;
    int m_currentBatch = -1;
};

} // namespace rustcam
