#pragma once

#include <QWidget>
#include <QTimer>
#include <QPainter>
#include <QPainterPath>
#include <QtMath>

namespace rustcam {

class AnimatedBackground : public QWidget {
    Q_OBJECT
public:
    AnimatedBackground(QWidget* parent = nullptr);
    ~AnimatedBackground();

    void stopAnimation();

private slots:
    void onTick();

private:
    QTimer* m_timer;
    qreal m_phase = 0.0;

protected:
    void paintEvent(QPaintEvent* event) override;
};

}