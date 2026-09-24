#pragma once

#include <QStyledItemDelegate>
#include <QStyleOptionViewItem>
#include <QPainter>
#include <QModelIndex>

namespace rustcam {

class FullRowSelectDelegate : public QStyledItemDelegate {
public:
    explicit FullRowSelectDelegate(QWidget* parent = nullptr) : QStyledItemDelegate(parent) {}

    void paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override {
        QStyleOptionViewItem opt = option;

        if (opt.state & QStyle::State_Selected) {
            QPalette::ColorGroup cg = (opt.state & QStyle::State_Active)
                ? QPalette::Normal : QPalette::Inactive;
            painter->fillRect(opt.rect, opt.palette.color(cg, QPalette::Highlight));
            opt.state &= ~QStyle::State_Selected;
            opt.text = QString();
            QStyledItemDelegate::paint(painter, opt, index);
        } else {
            QStyledItemDelegate::paint(painter, opt, index);
        }
    }
};

}