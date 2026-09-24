#pragma once

#include <QString>
#include <QList>
#include <QJsonObject>
#include <QJsonArray>
#include "CoreExport.h"

struct RUSTCAM_CORE_API LayoutItem {
    QString id;
    int row = 0;
    int col = 0;
    int rowSpan = 1;
    int colSpan = 1;
    QString label;

    static LayoutItem fromJson(const QJsonObject& obj);
    QJsonObject toJson() const;
};

struct RUSTCAM_CORE_API PlayerLayout {
    int id = 0;
    QString name;
    int rows = 2;
    int cols = 2;
    QList<LayoutItem> layoutJson;
    bool isDefault = false;

    static PlayerLayout fromJson(const QJsonObject& obj);
    QJsonObject toJson() const;
};
