#pragma once

#include <QWidget>
#include <QList>
#include <QPushButton>
#include "PlayerLayout.h"

class QSignalMapper;

namespace rustcam {

class RUSTCAM_CORE_API ReplayNavBar : public QWidget {
    Q_OBJECT
public:
    explicit ReplayNavBar(QWidget* parent = nullptr);
    ~ReplayNavBar();

    void setLayouts(const QList<PlayerLayout>& layouts);
    QString generateLayoutSvg(const PlayerLayout& layout, int thumbW, int thumbH);

signals:
    void layoutSelected(int index);

protected:
    void resizeEvent(QResizeEvent* event) override;

private slots:
    /// 某个布局按钮被点击（序号由按钮自己带过来，不用 sender() 反查）
    void onButtonClicked(int index);

private:
    void updateButtonPositions();

    /// 按钮 -> 布局序号
    QSignalMapper* m_indexMapper = nullptr;
    QList<QPushButton*> m_buttons;
    QList<PlayerLayout> m_layouts;
    int m_selectedIndex = -1;
};

}
