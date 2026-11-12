#pragma once

#include <QHash>
#include <QMap>
#include <QString>
#include <QVector>
#include <QWidget>

#include "TourScheme.h"

class QGridLayout;
class QLabel;
class QToolButton;

namespace rustcam {

/// 布局缩略图：按布局的行列与跨度画出窗口网格。
///
/// 用途是"直接选择布局中的窗口"——布局窗口的 id 是设计器生成的 uuid，
/// 展示给用户没有意义，这里按窗口在布局中的位置给出可读名称
/// （见 layoutItemDisplayName），点击格子即可选中该窗口。
/// 已设为局部展示（锁定）的窗口会高亮，并在格子里显示通道名。
class TourLayoutPicker : public QWidget {
    Q_OBJECT
public:
    explicit TourLayoutPicker(QWidget* parent = nullptr);

    /// 设置要预览的布局
    void setLayout(const PlayerLayout& layout);

    /// 设置局部展示清单。channelNames 的键是 TourChannelRef::key()（deviceTag/channelTag），
    /// 值是通道的显示名。
    void setLocks(const QVector<LockedSlot>& locks, const QHash<QString, QString>& channelNames);

    /// 当前选中的窗口 id，未选中时为空
    QString selectedItemId() const { return m_selectedItemId; }
    /// 选中指定窗口（未知窗口 id 时忽略）
    void selectItem(const QString& layoutItemId);

signals:
    /// 用户点选了某个窗口
    void itemSelected(const QString& layoutItemId);

private:
    struct CellWidgets;

    /// 按当前布局重建格子
    void rebuildCells();
    /// 按当前锁定状态刷新格子显示
    void refreshCells();

    QGridLayout* m_grid = nullptr;
    PlayerLayout m_layout;
    QVector<LockedSlot> m_locks;
    QHash<QString, QString> m_channelNames;
    QMap<QString, CellWidgets*> m_cells;
    QString m_selectedItemId;
};

} // namespace rustcam
