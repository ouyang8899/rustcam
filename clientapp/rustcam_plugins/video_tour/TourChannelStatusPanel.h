#pragma once

#include <QWidget>

class QLabel;
class QTableView;

namespace rustcam {

class TourChannelModel;

/// 左栏下部的通道状态列表：标题行 + 通道状态表。
///
/// 「重试」不在这里：它对状态表中当前选中的通道生效，属于运行页的动作，
/// 放在视频窗口下方，用户点之前能看见正在轮询的方案。
class TourChannelStatusPanel : public QWidget {
    Q_OBJECT
public:
    explicit TourChannelStatusPanel(QWidget* parent = nullptr);

    /// 状态表模型（面板持有）。
    TourChannelModel* model() const { return m_model; }
    /// 当前选中行；未选中返回 -1。
    int selectedRow() const;

private:
    TourChannelModel* m_model = nullptr;
    QTableView* m_tableView = nullptr;
    QLabel* m_titleLabel = nullptr;
};

} // namespace rustcam
