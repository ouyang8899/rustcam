#pragma once

#include <QJsonObject>
#include <QString>
#include <StyledWidget.h>

class QLineEdit;
class QPushButton;
class QLabel;
class QCheckBox;
class QSpinBox;

namespace rustcam {

class AnimatedBackground;

class LoginWindow : public StyledWidget {
    Q_OBJECT
public:
    explicit LoginWindow(QWidget* parent = nullptr);
    ~LoginWindow();

signals:
    void loginSuccess(const QJsonObject& userInfo);

private slots:
    void onLoginClicked();
    void onUsernameChanged(const QString& text);
    void onPasswordChanged(const QString& text);
    void onAuthSuccess(const QJsonObject& data);
    void onAuthFailed(const QString& error);
    void onOpenServerConfig();

private:
    void setupUi();
    void setupConnections();
    void loadServerConfig();
    void saveServerConfig();
    void setLoading(bool loading);
    bool validateInput();
    void showError(const QString& message);
    void clearError();

    QLineEdit* m_usernameEdit;
    QLineEdit* m_passwordEdit;
    QLineEdit* m_serverHostEdit;
    QSpinBox* m_serverPortSpin;
    QCheckBox* m_httpsCheck;
    QPushButton* m_loginButton;
    QLabel* m_errorLabel;
    QCheckBox* m_rememberCheck;
    QLabel* m_serverConfigLink;
    AnimatedBackground* m_animationBg;

    bool m_loading;
};

}
