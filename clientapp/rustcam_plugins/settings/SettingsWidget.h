#pragma once

#include <QWidget>
#include <QCheckBox>
#include <QComboBox>
#include <QSpinBox>
#include <QTabWidget>
#include <QColor>
#include <QColorDialog>
#include <QListWidget>
#include <QLabel>
#include <QLineEdit>

namespace rustcam {

class SettingsWidget : public QWidget {
    Q_OBJECT
public:
    explicit SettingsWidget(QWidget* parent = nullptr);
    ~SettingsWidget();

private slots:
    void onMediaRateVisibleChanged(bool checked);
    void onMediaRateAlignChanged(int index);
    void onDeviceNameVisibleChanged(bool checked);
    void onDeviceNameAlignChanged(int index);
    void onTimeVisibleChanged(bool checked);
    void onTimeAlignChanged(int index);
    void onTimeFormatChanged(int index);
    void onWatermarkVisibleChanged(bool checked);
    void onWatermarkTextChanged(const QString& text);
    void onOsdFontSizeChanged(int value);
    void onOsdFontColorClicked();
    void onOsdBgAlphaChanged(int value);
    void onPluginOrderChanged();
    void onPluginMoveUp();
    void onPluginMoveDown();

private:
    void loadSettings();
    void setupOsdDisplayTab();
    void setupOsdStyleTab();
    void setupPluginTab();
    void refreshPluginList();

    QTabWidget* m_tabWidget;

    QCheckBox* m_mediaRateVisibleCheck;
    QComboBox* m_mediaRateAlignCombo;
    QCheckBox* m_deviceNameVisibleCheck;
    QComboBox* m_deviceNameAlignCombo;
    QCheckBox* m_timeVisibleCheck;
    QComboBox* m_timeAlignCombo;
    QComboBox* m_timeFormatCombo;
    QCheckBox* m_watermarkVisibleCheck;
    QLineEdit* m_watermarkTextEdit;

    QSpinBox* m_osdFontSizeSpin;
    QPushButton* m_osdFontColorBtn;
    QLabel* m_osdFontColorLabel;
    QSpinBox* m_osdBgAlphaSpin;
    QColor m_osdFontColor;

    QListWidget* m_pluginOrderList;
};

}
