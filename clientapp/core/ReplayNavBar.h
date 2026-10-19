#pragma once

#include <QWidget>
#include <QList>
#include <QPushButton>
#include "PlayerLayout.h"

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
    void onButtonClicked();

private:
    void updateButtonPositions();

    QList<QPushButton*> m_buttons;
    QList<PlayerLayout> m_layouts;
    int m_selectedIndex = -1;
};

}
