#include "ThemeManager.h"
#include "Logger.h"
#include <QFile>
#include <QTextStream>
#include <QRegularExpression>
#include <QApplication>
#include <QPalette>
#include <QColor>
#include <QFont>
#include <QDir>
#include <QPixmapCache>
#include <QWidget>
#include <QtWidgets>

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
#ifdef _DEBUG
    m_themeDir = "D:/RustCam-Media/clientapp/resources/qss";
#else
    QString basePath = qApp->applicationDirPath();
    m_themeDir = basePath + "/resources/qss";
#endif

    LOG_INFO("ThemeManager", QString("Theme dir: %1").arg(m_themeDir));

    QDir themeDir(m_themeDir);
    if (!themeDir.exists()) {
        LOG_ERROR("ThemeManager", QString("Theme directory does not exist: %1").arg(m_themeDir));
    } else {
        QStringList files = themeDir.entryList(QStringList() << "*.qss");
        LOG_INFO("ThemeManager", QString("Found QSS files: %1").arg(files.join(", ")));
    }
}

QString ThemeManager::themeName(Theme theme) const
{
    switch (theme) {
        case Theme::None:         return "None";
        case Theme::MidnightPro:   return "Midnight Pro";
        case Theme::ArcticClean:   return "Arctic Clean";
        case Theme::ObsidianNeon: return "Obsidian Neon";
    }
    return "Unknown";
}

QString ThemeManager::themeFilePath(Theme theme) const
{
    switch (theme) {
        case Theme::None:         return QString();
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
    LOG_INFO("ThemeManager", QString("setTheme called: %1, current: %2").arg(themeName(theme)).arg(themeName(m_currentTheme)));

    QString filePath = themeFilePath(theme);
    if (filePath.isEmpty() || !QFile::exists(filePath)) {
        LOG_ERROR("ThemeManager", QString("Theme file not found: %1").arg(filePath));
        return;
    }

    QFile file(filePath);
    if (!file.open(QFile::ReadOnly | QFile::Text)) {
        LOG_ERROR("ThemeManager", QString("Failed to open theme file: %1").arg(filePath));
        return;
    }

    QTextStream stream(&file);
    QString qss = stream.readAll();
    file.close();

    LOG_INFO("ThemeManager", QString("QSS loaded, size: %1 bytes, preview: %2")
        .arg(qss.size()).arg(qss.left(100)));

    QPixmapCache::clear();

    applyStylesheet(qss);
    LOG_INFO("ThemeManager", QString("Stylesheet applied to qApp, isEmpty: %1")
        .arg(qApp->styleSheet().isEmpty()));
    m_currentTheme = theme;

    for (QWidget* w : QApplication::topLevelWidgets()) {
        w->setAttribute(Qt::WA_StyledBackground, true);
        w->style()->unpolish(w);
        w->style()->polish(w);
        w->update();
    }

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
    QRegularExpression defRe(R"(@(\w+)\s*:\s*([^;]+);)");

    QMap<QString, QString> vars;
    QRegularExpressionMatchIterator defIt = defRe.globalMatch(qss);
    while (defIt.hasNext()) {
        QRegularExpressionMatch match = defIt.next();
        vars[match.captured(1)] = match.captured(2).trimmed();
    }

    QString processedQss = qss;
    processedQss.remove(defRe);

    for (auto it = vars.constBegin(); it != vars.constEnd(); ++it) {
        processedQss.replace("@" + it.key(), it.value());
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
