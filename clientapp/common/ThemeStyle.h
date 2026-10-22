#pragma once

#include <QProxyStyle>
#include <QStyleOptionComboBox>
#include <QPainterPath>
#include <QPainter>
#include <QPalette>
#include <QComboBox>

namespace rustcam {

class ThemeStyle : public QProxyStyle {
public:
    explicit ThemeStyle(QStyle* baseStyle = nullptr) : QProxyStyle(baseStyle ? baseStyle : QStyleFactory::create("Fusion")) {}

    void drawComplexControl(ComplexControl control, const QStyleOptionComplex* option,
                            QPainter* painter, const QWidget* widget = nullptr) const override {
        if (control == CC_ComboBox && widget && widget->inherits("QComboBox")) {
            QProxyStyle::drawComplexControl(control, option, painter, widget);

            const QStyleOptionComboBox* comboOpt = qstyleoption_cast<const QStyleOptionComboBox*>(option);
            if (!comboOpt) return;

            QRect arrowRect = subControlRect(CC_ComboBox, comboOpt, SC_ComboBoxArrow, widget);
            if (arrowRect.isNull()) return;

            QPalette palette = widget->palette();
            QColor arrowColor = palette.text().color();

            painter->save();
            painter->setRenderHint(QPainter::Antialiasing);

            QPainterPath path;
            qreal w = arrowRect.width() - 4;
            qreal h = arrowRect.height() * 0.5;
            qreal x = arrowRect.x() + 2;
            qreal y = arrowRect.y() + (arrowRect.height() - h) / 2;

            path.moveTo(x, y);
            path.lineTo(x + w / 2, y + h);
            path.lineTo(x + w, y);

            painter->setPen(Qt::NoPen);
            painter->setBrush(arrowColor);
            painter->drawPath(path);
            painter->restore();
            return;
        }
        QProxyStyle::drawComplexControl(control, option, painter, widget);
    }
};

}
