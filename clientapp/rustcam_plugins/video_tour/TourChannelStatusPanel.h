#pragma once

#include <QWidget>

class QLabel;
class QPushButton;
class QTableView;

namespace rustcam {

class TourChannelModel;

/// 左栏下部的通道状态列表：标题行（含重试按钮）+ 通道状态表。
class TourChannelStatusPanel : public QWidget {
    Q_OBJECT
public:
    explicit TourChannelStatusPanel(QWidget* parent = nullptr);

    /// 状态表模型（面板持有）。
    TourChannelModel* model() const { return m_model; }
    /// 当前选中行；未选中返回 -1。
    int selectedRow() const;
    /// 重试按钮的可用状态。
    void setRetryEnabled(bool enabled);

signals:
    /// 用户点击重试，参数为当前选中行（未选中为 -1）。
    void retryRequested(int row);

private slots:
    void onRetryClicked();

private:
    TourChannelModel* m_model = nullptr;
    QTableView* m_tableView = nullptr;
    QLabel* m_titleLabel = nullptr;
    QPushButton* m_retryButton = nullptr;
};

} // namespace rustcam
