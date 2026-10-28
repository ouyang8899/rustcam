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
#define TEST_PATH 1
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
#ifdef TEST_PATH
    m_themeDir = "D:/RustCam-Media/clientapp/resources/qss";
#else
    // The build copies the QSS files to <bin>/resources/qss while the
    // executable lives in <bin>/<Config>/, so look next to the executable
    // first and then one level up.
    const QString basePath = qApp->applicationDirPath();
    const QStringList candidates = {
        basePath + "/resources/qss",
        QDir::cleanPath(basePath + "/../resources/qss"),
    };

    m_themeDir = candidates.first();
    for (const QString& candidate : candidates) {
        if (QDir(candidate).exists()) {
            m_themeDir = candidate;
            break;
        }
    }
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

    // 保存解析后的变量，供自绘控件（时间轴、录像列表等）取色
    m_styleValues = vars;

    QString processedQss = qss;
    processedQss.remove(defRe);

    for (auto it = vars.constEnd(); it != vars.constBegin(); ) {
        --it;
        processedQss.replace("@" + it.key(), it.value());
    }

    LOG_INFO("ThemeManager", QString("QSS processed length: %1, remaining @ count: %2")
        .arg(processedQss.length()).arg(processedQss.count('@')));
    int itemIdx = processedQss.indexOf("QTreeView::item");
    if (itemIdx >= 0) {
        LOG_INFO("ThemeManager", QString("QTreeView::item context: %1")
            .arg(processedQss.mid(itemIdx, 120)));
    }

    qApp->setStyleSheet(processedQss);
}

QString ThemeManager::getStyleValue(const QString& key, const QString& defaultValue) const
{
    return m_styleValues.value(key, defaultValue);
}

QColor ThemeManager::getColor(const QString& key) const
{
    const QString value = m_styleValues.value(key).trimmed();
    if (value.isEmpty()) {
        return QColor();
    }

    if (value.startsWith("#")) {
        return QColor(value);
    }

    // 支持 rgba(0, 120, 212, 0.25) 这类 QSS 颜色
    static const QRegularExpression rgbaRe(
        R"(^rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*(?:,\s*(\d*\.?\d+)\s*)?\)$)",
        QRegularExpression::CaseInsensitiveOption);
    const QRegularExpressionMatch match = rgbaRe.match(value);
    if (match.hasMatch()) {
        double alpha = 1.0;
        if (!match.captured(4).isEmpty()) {
            alpha = match.captured(4).toDouble();
            if (alpha > 1.0) {
                alpha /= 255.0;
            }
        }
        return QColor(match.captured(1).toInt(),
                      match.captured(2).toInt(),
                      match.captured(3).toInt(),
                      qRound(alpha * 255.0));
    }

    return QColor(value);
}

QColor ThemeManager::getColor(const QString& key, const QColor& defaultValue) const
{
    const QColor color = getColor(key);
    return color.isValid() ? color : defaultValue;
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
