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

/// 左侧方案列表：新建 / 编辑 / 删除 / 开始。
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
    void startRequested(const TourScheme& scheme);
    void schemesChanged();
    void notify(const QString& message, bool isError);

private slots:
    void onNewScheme();
    void onEditScheme();
    void onDeleteScheme();
    void onStartScheme();
    void onItemActivated(QListWidgetItem* item);
    void onSelectionChanged();

private:
    TourScheme* selectedScheme() const;

    TourApi* m_api = nullptr;
    QListWidget* m_list = nullptr;
    QPushButton* m_newButton = nullptr;
    QPushButton* m_editButton = nullptr;
    QPushButton* m_deleteButton = nullptr;
    QPushButton* m_startButton = nullptr;
    QVector<TourScheme> m_schemes;
    QList<PlayerLayout> m_layouts;
};

} // namespace rustcam
