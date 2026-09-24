#include "PlayerLayout.h"

LayoutItem LayoutItem::fromJson(const QJsonObject& obj) {
    LayoutItem item;
    item.id = obj["id"].toString();
    item.row = obj["row"].toInt();
    item.col = obj["col"].toInt();
    item.rowSpan = obj["row_span"].toInt(1);
    item.colSpan = obj["col_span"].toInt(1);
    item.label = obj["label"].toString();
    return item;
}

QJsonObject LayoutItem::toJson() const {
    QJsonObject obj;
    obj["id"] = id;
    obj["row"] = row;
    obj["col"] = col;
    obj["row_span"] = rowSpan;
    obj["col_span"] = colSpan;
    if (!label.isEmpty()) obj["label"] = label;
    return obj;
}

PlayerLayout PlayerLayout::fromJson(const QJsonObject& obj) {
    PlayerLayout layout;
    layout.id = obj["id"].toInt();
    layout.name = obj["name"].toString();
    layout.rows = obj["rows"].toInt();
    layout.cols = obj["cols"].toInt();
    layout.isDefault = obj["is_default"].toBool();

    QJsonArray json = obj["layout_json"].toArray();
    for (const QJsonValue& v : json) {
        layout.layoutJson.append(LayoutItem::fromJson(v.toObject()));
    }
    return layout;
}

QJsonObject PlayerLayout::toJson() const {
    QJsonObject obj;
    obj["id"] = id;
    obj["name"] = name;
    obj["rows"] = rows;
    obj["cols"] = cols;
    obj["is_default"] = isDefault;

    QJsonArray json;
    for (const auto& item : layoutJson) {
        json.append(item.toJson());
    }
    obj["layout_json"] = json;
    return obj;
}
