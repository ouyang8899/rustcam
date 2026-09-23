#include "AnimatedBackground.h"
#include "ThemeManager.h"

namespace rustcam {

AnimatedBackground::AnimatedBackground(QWidget* parent)
    : QWidget(parent)
{
    m_timer = new QTimer(this);
    m_timer->setInterval(16);
    connect(m_timer, &QTimer::timeout, this, &AnimatedBackground::onTick);
    m_timer->start();
}

AnimatedBackground::~AnimatedBackground()
{
    m_timer->stop();
}

void AnimatedBackground::stopAnimation()
{
    m_timer->stop();
}

void AnimatedBackground::onTick()
{
    m_phase += 0.003;
    repaint();
}

void AnimatedBackground::paintEvent(QPaintEvent* event)
{
    Q_UNUSED(event)
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);

    ThemeManager* tm = ThemeManager::instance();
    QColor primaryBg = tm->getColor("primary_bg");
    QColor secondaryBg = tm->getColor("secondary_bg");
    QColor accent = tm->getColor("accent");
    QColor accentHover = tm->getColor("accent_hover");
    QColor textPrimary = tm->getColor("text_primary");
    QColor textSecondary = tm->getColor("text_secondary");

    if (!primaryBg.isValid()) primaryBg = QColor(22, 33, 62);
    if (!secondaryBg.isValid()) secondaryBg = QColor(30, 30, 46);
    if (!accent.isValid()) accent = QColor(0, 120, 212);
    if (!accentHover.isValid()) accentHover = QColor(0, 150, 255);

    QLinearGradient bgGrad(0, 0, 0, height());
    bgGrad.setColorAt(0, primaryBg.darker(180));
    bgGrad.setColorAt(1, secondaryBg.darker(150));
    painter.fillRect(rect(), bgGrad);

    QColor gridColor = accent;
    gridColor.setAlpha(15);
    painter.setPen(QPen(gridColor, 1));
    int gridStep = 40;
    qreal offset = std::fmod(m_phase * gridStep, gridStep);
    for (int x = -gridStep + static_cast<int>(offset); x < width() + gridStep; x += gridStep) {
        painter.drawLine(x, 0, x, height());
    }
    for (int y = -gridStep + static_cast<int>(offset * 0.5); y < height() + gridStep; y += gridStep) {
        painter.drawLine(0, y, width(), y);
    }

    QColor accentLight = accent;
    accentLight.setAlpha(80);

    QPainterPath wave1;
    wave1.moveTo(0, height());
    for (int x = 0; x <= width(); x += 3) {
        qreal y = height() * 0.7 + qSin((x * 0.01) + m_phase * 6.28318) * 15;
        wave1.lineTo(x, y);
    }
    wave1.lineTo(width(), height());
    wave1.closeSubpath();

    QLinearGradient waveGrad1(0, height() * 0.65, 0, height());
    waveGrad1.setColorAt(0, QColor(accent.red(), accent.green(), accent.blue(), 40));
    waveGrad1.setColorAt(1, accent.darker(180));
    painter.fillPath(wave1, waveGrad1);

    QPainterPath wave2;
    wave2.moveTo(0, height());
    for (int x = 0; x <= width(); x += 3) {
        qreal y = height() * 0.75 + qSin((x * 0.008) + m_phase * 4.0 + 1.5) * 20;
        wave2.lineTo(x, y);
    }
    wave2.lineTo(width(), height());
    wave2.closeSubpath();

    QLinearGradient waveGrad2(0, height() * 0.7, 0, height());
    waveGrad2.setColorAt(0, accent.darker(250));
    waveGrad2.setColorAt(1, accent.darker(180));
    painter.fillPath(wave2, waveGrad2);

    for (int i = 0; i < 12; ++i) {
        qreal baseX = width() * (0.08 + i * 0.08);
        qreal baseY = height() * 0.3 + qSin(i * 0.8) * 50;
        qreal orbitR = 20 + qSin(m_phase * 3.0 + i) * 8;
        qreal cx = baseX + qCos(m_phase * 2.0 + i * 0.5) * orbitR;
        qreal cy = baseY + qSin(m_phase * 2.5 + i * 0.7) * orbitR * 0.4;

        QRadialGradient glow(cx, cy, 15);
        glow.setColorAt(0, QColor(accent.red(), accent.green(), accent.blue(), 50));
        glow.setColorAt(1, QColor(accent.red(), accent.green(), accent.blue(), 0));
        painter.fillRect(cx - 15, cy - 15, 30, 30, glow);

        painter.setPen(QPen(accentLight, 1.5));
        painter.drawEllipse(QPointF(cx, cy), 3, 3);
    }

    for (int i = 0; i < 25; ++i) {
        qreal sx = width() * 0.05 + std::fmod(i * 137.0, width() * 0.9);
        qreal sy = std::fmod(i * 89.0 + std::fmod(m_phase, 1.0) * 30, height() * 0.65);
        qreal ex = sx + 30 + std::fmod(i * 17.0, 50.0);
        qreal ey = sy + std::fmod(i * 23.0, 30.0);

        QLinearGradient lineGrad(sx, sy, ex, ey);
        lineGrad.setColorAt(0, QColor(accent.red(), accent.green(), accent.blue(), 30));
        lineGrad.setColorAt(1, QColor(accent.red(), accent.green(), accent.blue(), 0));
        painter.setPen(QPen(lineGrad, 1));
        painter.drawLine(QPointF(sx, sy), QPointF(ex, ey));
    }
}

}