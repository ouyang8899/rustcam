#include "ReplayNavBar.h"
#include <QVBoxLayout>
#include <QPushButton>
#include <QPainter>
#include <QStyleOption>
#include <QSvgRenderer>
#include <QIcon>
#include <QGuiApplication>

namespace rustcam {

ReplayNavBar::ReplayNavBar(QWidget* parent)
    : QWidget(parent)
{
    setFixedHeight(50);
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
    setStyleSheet(R"(
        ReplayNavBar {
            background-color: #1e1e1e;
        }
    )");
}

ReplayNavBar::~ReplayNavBar() {
}

void ReplayNavBar::setLayouts(const QList<PlayerLayout>& layouts) {
    m_layouts = layouts;

    for (QPushButton* btn : m_buttons) {
        btn->deleteLater();
    }
    m_buttons.clear();
    m_selectedIndex = -1;

    const int thumbW = 50;
    const int thumbH = 36;

    for (int i = 0; i < layouts.size(); ++i) {
        QPushButton* btn = new QPushButton(this);
        btn->setFixedSize(60, 44);
        btn->setCheckable(true);
        btn->setAutoExclusive(true);
        btn->setToolTip(layouts[i].name);

        QString svg = generateLayoutSvg(layouts[i], thumbW, thumbH);
        QSvgRenderer renderer(svg.toUtf8());
        if (renderer.isValid()) {
            QPixmap pixmap(thumbW, thumbH);
            pixmap.fill(Qt::transparent);
            QPainter painter(&pixmap);
            renderer.render(&painter);
            QIcon icon(pixmap);
            btn->setIcon(icon);
            btn->setIconSize(QSize(thumbW, thumbH));
        }

        btn->setProperty("class", "LayoutNavButton");
        btn->show();

        connect(btn, &QPushButton::clicked, this, &ReplayNavBar::onButtonClicked);
        btn->setProperty("index", i);
        m_buttons.append(btn);
    }

    // 默认选中第一个布局（容器本身也会用第一个布局初始化）
    if (!m_buttons.isEmpty()) {
        m_selectedIndex = 0;
        m_buttons[0]->setChecked(true);
    }

    updateButtonPositions();
}

QString ReplayNavBar::generateLayoutSvg(const PlayerLayout& layout, int thumbW, int thumbH) {
    QString svg = QString(
        "<svg xmlns='http://www.w3.org/2000/svg' width='%1' height='%2' viewBox='0 0 %1 %2'>"
    ).arg(thumbW).arg(thumbH);

    svg += QString("<rect width='100%25' height='100%25' fill='%1'/>").arg("#2a2a2a");

    int margin = 2;
    int spacing = 1;
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

void ReplayNavBar::onButtonClicked() {
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

void ReplayNavBar::resizeEvent(QResizeEvent* event) {
    QWidget::resizeEvent(event);
    updateButtonPositions();
}

void ReplayNavBar::updateButtonPositions() {
    int totalWidth = m_buttons.size() * 70 - 10;
    int startX = (width() - totalWidth) / 2;

    for (int i = 0; i < m_buttons.size(); ++i) {
        m_buttons[i]->move(startX + i * 70, 5);
    }
}

}
