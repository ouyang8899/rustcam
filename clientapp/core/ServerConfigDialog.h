#pragma once

#include <QDialog>
#include <QJsonObject>

class QLineEdit;
class QSpinBox;
class QCheckBox;
class QPushButton;
class QLabel;

namespace rustcam {

class ServerConfigDialog : public QDialog {
    Q_OBJECT
public:
    explicit ServerConfigDialog(QWidget* parent = nullptr);
    ~ServerConfigDialog();

private slots:
    void onSaveClicked();
    void onTestClicked();

private:
    void setupUi();
    void loadCurrentConfig();

    QLineEdit* m_serverHostEdit;
    QSpinBox* m_serverPortSpin;
    QCheckBox* m_httpsCheck;
    QPushButton* m_saveButton;
    QPushButton* m_testButton;
    QPushButton* m_cancelButton;
    QLabel* m_statusLabel;
};

}
