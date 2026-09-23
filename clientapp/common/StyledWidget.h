#pragma once

#include <QWidget>
#include <QStyleOption>
#include <QPainter>

namespace rustcam {

class StyledWidget : public QWidget {
    Q_OBJECT
public:
    explicit StyledWidget(QWidget* parent = nullptr) : QWidget(parent) {
        setAttribute(Qt::WA_StyledBackground);
    }

protected:
    void paintEvent(QPaintEvent*) override {
        QStyleOption opt;
        opt.init(this);
        QPainter p(this);
        style()->drawPrimitive(QStyle::PE_Widget, &opt, &p, this);
    }
};

}
