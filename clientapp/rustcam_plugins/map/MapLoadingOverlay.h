#pragma once

#include <QWidget>

class QLabel;
class QProgressBar;
class QPushButton;

namespace rustcam {

/// 覆盖在地图容器之上的加载/失败提示。
///
/// 远端 CEF 窗口是原生子窗口，普通 Qt 兄弟控件会被它盖住，因此本控件自身
/// 也是原生窗口，作为 mapContainer 的兄弟参与 z-order，并显式 raise()。
class MapLoadingOverlay : public QWidget {
    Q_OBJECT
public:
    explicit MapLoadingOverlay(QWidget* parent);

    void showBusy(const QString& text);
    void showError(const QString& text);

signals:
    void retryRequested();

protected:
    bool eventFilter(QObject* watched, QEvent* event) override;
    void showEvent(QShowEvent* event) override;

private:
    void syncGeometry();

    QLabel* m_text = nullptr;
    QProgressBar* m_progress = nullptr;
    QPushButton* m_retry = nullptr;
};

} // namespace rustcam
