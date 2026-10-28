#pragma once

#include "ThemeManager.h"
#include <QStyledItemDelegate>
#include <QStyleOptionViewItem>
#include <QPainter>
#include <QModelIndex>
#include <QPalette>

namespace rustcam {

class FullRowSelectDelegate : public QStyledItemDelegate {
public:
    explicit FullRowSelectDelegate(QWidget* parent = nullptr) : QStyledItemDelegate(parent) {}

    void paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override {
        QStyleOptionViewItem opt = option;

        if (opt.state & QStyle::State_Selected) {
            // 选中背景跟随主题（含窗口失焦态），不要用系统调色板的灰色高亮
            const bool active = opt.state & QStyle::State_Active;
            const QPalette::ColorGroup cg = active ? QPalette::Active : QPalette::Inactive;
            const QColor highlight = ThemeManager::instance()->getColor(
                active ? "selected_bg_active" : "selected_bg",
                opt.palette.color(cg, QPalette::Highlight));
            painter->fillRect(opt.rect, highlight);
            opt.state &= ~QStyle::State_Selected;
            opt.text = QString();
            QStyledItemDelegate::paint(painter, opt, index);
        } else {
            QStyledItemDelegate::paint(painter, opt, index);
        }
    }
};

}