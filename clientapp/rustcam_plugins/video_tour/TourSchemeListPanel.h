#pragma once

#include "TourScheme.h"

#include <QList>
#include <QVector>
#include <QWidget>

#include "PlayerLayout.h"

class QListWidget;
class QListWidgetItem;
class QPushButton;

namespace rustcam {

class TourApi;

/// 左侧方案面板：顶部操作按钮（新建 / 编辑 / 删除）+ 下方方案树。
///
/// 「开始」不在这里：它按当前选中的方案启动轮询，属于运行页的动作，
/// 放在视频窗口下方，用户点之前能看见正在轮询的方案。
class TourSchemeListPanel : public QWidget {
    Q_OBJECT
public:
    explicit TourSchemeListPanel(TourApi* api, QWidget* parent = nullptr);

    /// 重新拉取方案列表。
    void refresh();
    /// 当前选中的方案；未选中时返回 false。
    bool currentScheme(TourScheme* out) const;
    /// 供编辑弹窗使用的布局列表。
    void setLayouts(const QList<PlayerLayout>& layouts) { m_layouts = layouts; }

signals:
    /// 选中项变化；未选中时 scheme 为空。
    void schemeSelected(const TourScheme& scheme);
    void schemesChanged();
    void notify(const QString& message, bool isError);

private slots:
    void onNewScheme();
    void onEditScheme();
    void onDeleteScheme();
    void onItemActivated(QListWidgetItem* item);
    void onSelectionChanged();

private:
    TourScheme* selectedScheme() const;

    TourApi* m_api = nullptr;
    QListWidget* m_list = nullptr;
    QPushButton* m_newButton = nullptr;
    QPushButton* m_editButton = nullptr;
    QPushButton* m_deleteButton = nullptr;
    QVector<TourScheme> m_schemes;
    QList<PlayerLayout> m_layouts;
};

} // namespace rustcam
