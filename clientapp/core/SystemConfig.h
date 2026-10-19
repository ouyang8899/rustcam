#pragma once

#include <QObject>
#include <QString>
#include <QVariant>
#include <QMap>
#include "CoreExport.h"

namespace rustcam {

class RUSTCAM_CORE_API SystemConfig : public QObject {
    Q_OBJECT
public:
    static SystemConfig* instance();

    QVariant getValue(const QString& key, const QVariant& defaultValue = QVariant()) const;
    void setValue(const QString& key, const QVariant& value);

    template<typename T>
    T getValue(const QString& key, const T& defaultValue) const {
        QVariant v = getValue(key);
        if (!v.isValid()) return defaultValue;
        return v.value<T>();
    }

    bool load();
    bool save();

    QStringList allKeys() const;

signals:
    void valueChanged(const QString& key, const QVariant& value);
    void configReloaded();

private:
    explicit SystemConfig(QObject* parent = nullptr);
    ~SystemConfig();
    Q_DISABLE_COPY(SystemConfig)

    bool initDatabase();
    void loadFromDb();
    void saveToDb(const QString& key, const QVariant& value);

    QMap<QString, QVariant> m_config;
};

}
