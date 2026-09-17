#pragma once

#include <QObject>
#include <QString>
#include <QVariant>
#include <QSettings>

namespace rustcam {

class ConfigManager : public QObject {
    Q_OBJECT
public:
    static ConfigManager* instance();

    void setValue(const QString& group, const QString& key, const QVariant& value);
    QVariant getValue(const QString& group, const QString& key, const QVariant& defaultValue = {}) const;

    void setGroup(const QString& group);
    QString currentGroup() const;

    void remove(const QString& group, const QString& key);
    void clearGroup(const QString& group);

    QStringList childGroups(const QString& group = {}) const;
    QStringList childKeys(const QString& group = {}) const;

    bool contains(const QString& group, const QString& key) const;

    void load(const QString& filePath);
    void save(const QString& filePath);

    void sync();

    QVariantMap getAllValues(const QString& group) const;

signals:
    void configChanged(const QString& group, const QString& key, const QVariant& value);

private:
    explicit ConfigManager(QObject* parent = nullptr);
    ~ConfigManager();
    Q_DISABLE_COPY(ConfigManager)

    QSettings* m_settings = nullptr;
    QString m_configFilePath;
};

}
