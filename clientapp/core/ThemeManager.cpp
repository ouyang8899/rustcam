#include "ThemeManager.h"
#include "Logger.h"
#include <QFile>
#include <QTextStream>
#include <QRegularExpression>
#include <QApplication>
#include <QPalette>
#include <QColor>
#include <QFont>

namespace rustcam {

ThemeManager::ThemeManager(QObject* parent)
    : QObject(parent)
{
    initThemePaths();
}

ThemeManager::~ThemeManager()
{
}

ThemeManager* ThemeManager::instance()
{
    static ThemeManager inst;
    return &inst;
}

void ThemeManager::initThemePaths()
{
    m_themeDir = qApp->applicationDirPath() + "/resources/qss";
}

QString ThemeManager::themeName(Theme theme) const
{
    switch (theme) {
        case Theme::MidnightPro:   return "Midnight Pro";
        case Theme::ArcticClean:   return "Arctic Clean";
        case Theme::ObsidianNeon: return "Obsidian Neon";
    }
    return "Unknown";
}

QString ThemeManager::themeFilePath(Theme theme) const
{
    switch (theme) {
        case Theme::MidnightPro:   return m_themeDir + "/midnight_pro.qss";
        case Theme::ArcticClean:   return m_themeDir + "/arctic_clean.qss";
        case Theme::ObsidianNeon: return m_themeDir + "/obsidian_neon.qss";
    }
    return QString();
}

QStringList ThemeManager::availableThemes() const
{
    return { "Midnight Pro", "Arctic Clean", "Obsidian Neon" };
}

void ThemeManager::setTheme(Theme theme)
{
    if (m_currentTheme == theme) {
        return;
    }

    QString filePath = themeFilePath(theme);
    if (filePath.isEmpty() || !QFile::exists(filePath)) {
        LOG_ERROR("ThemeManager", QString("Theme file not found: %1").arg(filePath));
        return;
    }

    loadTheme(filePath);
    m_currentTheme = theme;

    LOG_INFO("ThemeManager", QString("Theme changed to: %1").arg(themeName(theme)));
    emit themeChanged(theme);
}

void ThemeManager::loadTheme(const QString& qssFilePath)
{
    QFile file(qssFilePath);
    if (!file.open(QFile::ReadOnly | QFile::Text)) {
        LOG_ERROR("ThemeManager", QString("Failed to open theme file: %1").arg(qssFilePath));
        return;
    }

    QTextStream stream(&file);
    QString qss = stream.readAll();
    file.close();

    applyStylesheet(qss);
}

void ThemeManager::applyStylesheet(const QString& qss)
{
    QRegularExpression varRe(R"(@(\w+)\s*:\s*([^;]+);)");
    QRegularExpressionMatchIterator it = varRe.globalMatch(qss);

    QString processedQss = qss;
    while (it.hasNext()) {
        QRegularExpressionMatch match = it.next();
        QString key = match.captured(1);
        QString value = match.captured(2).trimmed();
        m_styleValues[key] = value;
        processedQss.replace("@" + key, value);
    }

    qApp->setStyleSheet(processedQss);
}

QString ThemeManager::getStyleValue(const QString& key, const QString& defaultValue) const
{
    return m_styleValues.value(key, defaultValue);
}

QColor ThemeManager::getColor(const QString& key) const
{
    QString value = m_styleValues.value(key);
    if (value.isEmpty()) {
        return QColor();
    }

    if (value.startsWith("#")) {
        return QColor(value);
    }

    return QColor(value);
}

QFont ThemeManager::getFont(const QString& key, const QFont& defaultFont) const
{
    Q_UNUSED(key)
    return defaultFont;
}

void ThemeManager::setPaletteColor(const QString& key, const QColor& color)
{
    QPalette palette = qApp->palette();
    if (key == "window") {
        palette.setColor(QPalette::Window, color);
    } else if (key == "windowText") {
        palette.setColor(QPalette::WindowText, color);
    }
    qApp->setPalette(palette);
}

}
