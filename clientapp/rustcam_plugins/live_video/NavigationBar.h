#pragma once

#include <QWidget>
#include <QList>
#include <QPushButton>
#include "core/PlayerLayout.h"

namespace rustcam {

class NavigationBar : public QWidget {
    Q_OBJECT
public:
    explicit NavigationBar(QWidget* parent = nullptr);
    ~NavigationBar();

    void setLayouts(const QList<PlayerLayout>& layouts);
    QString generateLayoutSvg(const PlayerLayout& layout, int thumbW, int thumbH);

signals:
    void layoutSelected(int index);

protected:
    void resizeEvent(QResizeEvent* event) override;

private slots:
    void onButtonClicked();

private:
    void updateButtonPositions();

    QList<QPushButton*> m_buttons;
    QList<PlayerLayout> m_layouts;
    int m_selectedIndex = -1;
};

}
