#pragma once

#include <QObject>
#include <QString>
#include <QMap>
#include <QStringList>
#include <QColor>
#include <QFont>

namespace rustcam {

class ThemeManager : public QObject {
    Q_OBJECT
public:
    static ThemeManager* instance();

    enum class Theme {
        MidnightPro,
        ArcticClean,
        ObsidianNeon
    };
    Q_ENUM(Theme)

    void setTheme(Theme theme);
    Theme currentTheme() const { return m_currentTheme; }

    QString themeName(Theme theme) const;
    QString themeFilePath(Theme theme) const;

    QStringList availableThemes() const;

    void loadTheme(const QString& qssFilePath);
    void applyStylesheet(const QString& qss);

    QString getStyleValue(const QString& key, const QString& defaultValue = QString()) const;

    QColor getColor(const QString& key) const;
    QFont getFont(const QString& key, const QFont& defaultFont = QFont()) const;

    void setPaletteColor(const QString& key, const QColor& color);

signals:
    void themeChanged(Theme theme);

private:
    explicit ThemeManager(QObject* parent = nullptr);
    ~ThemeManager();
    Q_DISABLE_COPY(ThemeManager)

    void initThemePaths();

    Theme m_currentTheme = Theme::MidnightPro;
    QString m_themeDir;
    QMap<QString, QString> m_styleValues;
};

}
