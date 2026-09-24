#include "NavigationBar.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QPushButton>
#include <QPainter>
#include <QStyleOption>
#include <QGuiApplication>
#include <QWindow>

namespace rustcam {

NavigationBar::NavigationBar(QWidget* parent)
    : QWidget(parent)
{
    setFixedHeight(50);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
}

NavigationBar::~NavigationBar() {
}

void NavigationBar::setLayouts(const QList<PlayerLayout>& layouts) {
    m_layouts = layouts;

    for (QPushButton* btn : m_buttons) {
        btn->deleteLater();
    }
    m_buttons.clear();

    for (int i = 0; i < layouts.size(); ++i) {
        QPushButton* btn = new QPushButton(this);
        btn->setFixedSize(60, 40);
        btn->setToolTip(layouts[i].name);

        btn->setStyleSheet(QString(
            "QPushButton { border: 1px solid #555; border-radius: 4px; background: qlineargradient(x1:0, y1:0, x2:0, y2:1, stop:0 #333, stop:1 #222); }"
            "QPushButton:hover { border: 1px solid #888; background: qlineargradient(x1:0, y1:0, x2:0, y2:1, stop:0 #444, stop:1 #333); }"
            "QPushButton:pressed { border: 1px solid #4a9eff; background: qlineargradient(x1:0, y1:0, x2:0, y2:1, stop:0 #4a9eff, stop:1 #2d7ad4); }"
        ));

        connect(btn, &QPushButton::clicked, this, &NavigationBar::onButtonClicked);
        btn->setProperty("index", i);
        m_buttons.append(btn);
    }

    updateButtonPositions();
}

QString NavigationBar::generateLayoutSvg(const PlayerLayout& layout, int thumbW, int thumbH) {
    QString svg = QString(
        "<svg xmlns='http://www.w3.org/2000/svg' width='%1' height='%2' viewBox='0 0 %1 %2'>"
    ).arg(thumbW).arg(thumbH);

    svg += QString("<rect width='100%25' height='100%25' fill='%1'/>").arg("#2a2a2a");

    int margin = 2;
    int spacing = 2;
    int cellW = (thumbW - margin * 2 - spacing * (layout.cols - 1)) / layout.cols;
    int cellH = (thumbH - margin * 2 - spacing * (layout.rows - 1)) / layout.rows;

    for (const auto& item : layout.layoutJson) {
        int x = margin + item.col * (cellW + spacing);
        int y = margin + item.row * (cellH + spacing);
        int w = item.colSpan * cellW + (item.colSpan - 1) * spacing;
        int h = item.rowSpan * cellH + (item.rowSpan - 1) * spacing;

        svg += QString("<rect x='%1' y='%2' width='%3' height='%4' fill='%5' rx='1'/>"
        ).arg(x).arg(y).arg(w).arg(h).arg("#4a9eff");
    }

    svg += "</svg>";
    return svg;
}

void NavigationBar::onButtonClicked() {
    QPushButton* btn = qobject_cast<QPushButton*>(sender());
    if (btn) {
        int index = btn->property("index").toInt();
        if (m_selectedIndex != index) {
            if (m_selectedIndex >= 0 && m_selectedIndex < m_buttons.size()) {
                m_buttons[m_selectedIndex]->setChecked(false);
            }
            m_selectedIndex = index;
            btn->setChecked(true);
            emit layoutSelected(index);
        }
    }
}

void NavigationBar::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);
    updateButtonPositions();
}

void NavigationBar::updateButtonPositions() {
    int totalWidth = m_buttons.size() * 70 - 10;
    int startX = (width() - totalWidth) / 2;

    for (int i = 0; i < m_buttons.size(); ++i) {
        m_buttons[i]->move(startX + i * 70, 5);
    }
}

}
