#include "LoginWindow.h"
#include "AnimatedBackground.h"
#include "ServerApi.h"
#include "ServerConfig.h"
#include "ServerConfigDialog.h"
#include "SessionManager.h"
#include "Logger.h"
#include "ThemeManager.h"
#include <QJsonDocument>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QGridLayout>
#include <QStackedLayout>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QCheckBox>
#include <QSpacerItem>
#include <QTimer>
#include <QPainter>
#include <QPainterPath>
#include <QtMath>
#include <QCoreApplication>
#include <QSpinBox>
#include <cmath>

namespace rustcam {

LoginWindow::LoginWindow(QWidget* parent)
    : StyledWidget(parent)
    , m_loading(false)
    , m_animationBg(nullptr)
{
    setObjectName("LoginWindow");
    setWindowTitle("rustcam - Login");
    setMinimumSize(900, 600);
    resize(1000, 650);

    loadServerConfig();
    setupUi();
    setupConnections();
}

LoginWindow::~LoginWindow()
{
}

void LoginWindow::loadServerConfig()
{
    ServerConfig::instance()->load();
}

void LoginWindow::saveServerConfig()
{
    ServerConfig::instance()->setServerHost(m_serverHostEdit->text().trimmed());
    ServerConfig::instance()->setServerPort(m_serverPortSpin->value());
    ServerConfig::instance()->setUseHttps(m_httpsCheck->isChecked());
}

void LoginWindow::setupUi()
{
    setMinimumSize(800, 500);
    resize(1000, 600);

    QHBoxLayout* mainLayout = new QHBoxLayout(this);
    mainLayout->setContentsMargins(0, 0, 0, 0);
    mainLayout->setSpacing(0);

    QWidget* leftPanel = new QWidget(this);
    leftPanel->setMinimumWidth(400);
    leftPanel->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
    leftPanel->setAutoFillBackground(false);

    QStackedLayout* leftStack = new QStackedLayout(leftPanel);
    leftStack->setContentsMargins(0, 0, 0, 0);
    leftStack->setStackingMode(QStackedLayout::StackAll);

    m_animationBg = new AnimatedBackground(leftPanel);
    m_animationBg->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
    leftStack->addWidget(m_animationBg);
    m_animationBg->lower();

    QWidget* textOverlay = new QWidget(leftPanel);
    textOverlay->setAttribute(Qt::WA_TransparentForMouseEvents);
    textOverlay->setStyleSheet("background: transparent;");
    QVBoxLayout* textLayout = new QVBoxLayout(textOverlay);
    textLayout->setContentsMargins(50, 50, 50, 50);

    QLabel* titleLabel = new QLabel(textOverlay);
    titleLabel->setObjectName("loginTitle");
    titleLabel->setText(tr("rustcam"));

    QLabel* subtitleLabel = new QLabel(textOverlay);
    subtitleLabel->setObjectName("loginSubtitle");
    subtitleLabel->setText(tr("Intelligent Video Surveillance System"));

    QLabel* versionLabel = new QLabel(textOverlay);
    versionLabel->setObjectName("loginVersion");
    versionLabel->setText(tr("v1.0.0"));

    textLayout->addWidget(titleLabel);
    textLayout->addWidget(subtitleLabel);
    textLayout->addWidget(versionLabel);
    textLayout->addStretch();

    leftStack->addWidget(textOverlay);
    leftStack->setCurrentIndex(1);

    mainLayout->addWidget(leftPanel, 1);

    QWidget* rightPanel = new QWidget(this);
    rightPanel->setObjectName("rightPanel");
    QVBoxLayout* rightLayout = new QVBoxLayout(rightPanel);
    rightLayout->setContentsMargins(60, 40, 60, 40);

    QSpacerItem* topSpacer = new QSpacerItem(20, 40, QSizePolicy::Minimum, QSizePolicy::Fixed);
    rightLayout->addSpacerItem(topSpacer);

    QLabel* welcomeLabel = new QLabel(rightPanel);
    welcomeLabel->setObjectName("welcomeLabel");
    welcomeLabel->setText(tr("Welcome"));
    rightLayout->addWidget(welcomeLabel);

    QLabel* descLabel = new QLabel(rightPanel);
    descLabel->setObjectName("descLabel");
    descLabel->setText(tr("Please enter your credentials to login"));
    rightLayout->addWidget(descLabel);

    QGridLayout* formLayout = new QGridLayout();
    formLayout->setSpacing(12);

    QLabel* serverLabel = new QLabel(rightPanel);
    serverLabel->setObjectName("formLabel");
    serverLabel->setText(tr("Server Address"));
    formLayout->addWidget(serverLabel, 0, 0, 1, 2);

    m_serverHostEdit = new QLineEdit(rightPanel);
    m_serverHostEdit->setObjectName("serverHostEdit");
    m_serverHostEdit->setPlaceholderText(tr("Server IP or Domain"));
    m_serverHostEdit->setMinimumHeight(40);
    m_serverHostEdit->setText(ServerConfig::instance()->serverHost());
    formLayout->addWidget(m_serverHostEdit, 1, 0, 1, 1);

    m_serverPortSpin = new QSpinBox(rightPanel);
    m_serverPortSpin->setObjectName("serverPortSpin");
    m_serverPortSpin->setMinimum(1);
    m_serverPortSpin->setMaximum(65535);
    m_serverPortSpin->setValue(ServerConfig::instance()->serverPort());
    m_serverPortSpin->setMinimumHeight(40);
    formLayout->addWidget(m_serverPortSpin, 1, 1, 1, 1);

    m_httpsCheck = new QCheckBox(rightPanel);
    m_httpsCheck->setObjectName("httpsCheck");
    m_httpsCheck->setText(tr("Use HTTPS"));
    m_httpsCheck->setChecked(ServerConfig::instance()->useHttps());
    formLayout->addWidget(m_httpsCheck, 2, 0, 1, 2);

    QLabel* userLabel = new QLabel(rightPanel);
    userLabel->setObjectName("formLabel");
    userLabel->setText(tr("Username"));
    formLayout->addWidget(userLabel, 3, 0, 1, 2);

    m_usernameEdit = new QLineEdit(rightPanel);
    m_usernameEdit->setObjectName("usernameEdit");
    m_usernameEdit->setPlaceholderText(tr("Enter username"));
    m_usernameEdit->setMinimumHeight(40);
    formLayout->addWidget(m_usernameEdit, 4, 0, 1, 2);

    QLabel* pwdLabel = new QLabel(rightPanel);
    pwdLabel->setObjectName("formLabel");
    pwdLabel->setText(tr("Password"));
    formLayout->addWidget(pwdLabel, 5, 0, 1, 2);

    m_passwordEdit = new QLineEdit(rightPanel);
    m_passwordEdit->setObjectName("passwordEdit");
    m_passwordEdit->setPlaceholderText(tr("Enter password"));
    m_passwordEdit->setEchoMode(QLineEdit::Password);
    m_passwordEdit->setMinimumHeight(40);
    formLayout->addWidget(m_passwordEdit, 6, 0, 1, 2);

    rightLayout->addLayout(formLayout);

    QHBoxLayout* optionsLayout = new QHBoxLayout();
    optionsLayout->setSpacing(16);

    m_rememberCheck = new QCheckBox(rightPanel);
    m_rememberCheck->setObjectName("rememberCheck");
    m_rememberCheck->setText(tr("Remember Password"));
    m_rememberCheck->setChecked(true);
    optionsLayout->addWidget(m_rememberCheck);

    QLabel* forgotLabel = new QLabel(rightPanel);
    forgotLabel->setObjectName("forgotLabel");
    QColor textSecondary = ThemeManager::instance()->getColor("text_secondary");
    if (!textSecondary.isValid()) textSecondary = QColor(158, 158, 158);
    forgotLabel->setText(QString("<a href='#' style='color: %1;'>%2</a>")
        .arg(textSecondary.name()).arg(tr("Forgot Password?")));
    forgotLabel->setAlignment(Qt::AlignRight);
    forgotLabel->setOpenExternalLinks(false);
    optionsLayout->addWidget(forgotLabel, 1);

    rightLayout->addLayout(optionsLayout);

    m_errorLabel = new QLabel(rightPanel);
    m_errorLabel->setObjectName("errorLabel");
    m_errorLabel->setVisible(false);
    rightLayout->addWidget(m_errorLabel);

    m_loginButton = new QPushButton(rightPanel);
    m_loginButton->setObjectName("loginButton");
    m_loginButton->setText(tr("Login"));
    m_loginButton->setMinimumHeight(50);
    m_loginButton->setCursor(Qt::PointingHandCursor);
    rightLayout->addWidget(m_loginButton);

    QSpacerItem* bottomSpacer = new QSpacerItem(20, 12, QSizePolicy::Minimum, QSizePolicy::Fixed);
    rightLayout->addSpacerItem(bottomSpacer);

    QHBoxLayout* footerLayout = new QHBoxLayout();
    footerLayout->setSpacing(8);

    m_serverConfigLink = new QLabel(rightPanel);
    m_serverConfigLink->setObjectName("serverConfigLink");
    QColor linkColor = ThemeManager::instance()->getColor("text_secondary");
    if (!linkColor.isValid()) linkColor = QColor(158, 158, 158);
    m_serverConfigLink->setText(QString("<a href='#' style='color: %1;'>%2</a>")
        .arg(linkColor.name()).arg(tr("Server Settings")));
    m_serverConfigLink->setCursor(Qt::PointingHandCursor);
    footerLayout->addWidget(m_serverConfigLink);

    footerLayout->addStretch();

    QLabel* copyrightLabel = new QLabel(rightPanel);
    copyrightLabel->setObjectName("copyrightLabel");
    copyrightLabel->setText(tr("(c) 2026 rustcam. All rights reserved."));
    footerLayout->addWidget(copyrightLabel);

    rightLayout->addLayout(footerLayout);

    mainLayout->addWidget(rightPanel);

    serverLabel->setVisible(false);
    m_serverHostEdit->setVisible(false);
    m_serverPortSpin->setVisible(false);
    m_httpsCheck->setVisible(false);
}

void LoginWindow::setupConnections()
{
    connect(m_loginButton, &QPushButton::clicked, this, &LoginWindow::onLoginClicked);
    connect(m_usernameEdit, &QLineEdit::textChanged, this, &LoginWindow::onUsernameChanged);
    connect(m_passwordEdit, &QLineEdit::textChanged, this, &LoginWindow::onPasswordChanged);
    connect(m_serverConfigLink, &QLabel::linkActivated, this, &LoginWindow::onOpenServerConfig);

    connect(m_usernameEdit, &QLineEdit::returnPressed, this, &LoginWindow::onLoginClicked);
    connect(m_passwordEdit, &QLineEdit::returnPressed, this, &LoginWindow::onLoginClicked);
}

void LoginWindow::onLoginClicked()
{
    if (m_loading) return;

    clearError();

    if (!validateInput()) return;

    saveServerConfig();

    setLoading(true);

    QJsonObject body;
    body["username"] = m_usernameEdit->text().trimmed();
    body["password"] = m_passwordEdit->text();

    ServerApi::instance()->post("/api/v1/auth/login", body, [this](bool success, const QJsonObject& response, const QString& error) {
        if (success) {
            int code = response["code"].toInt();
            if (code == 200) {
                onAuthSuccess(response);
            } else {
                QString msg = response["message"].toString();
                onAuthFailed(msg.isEmpty() ? QString("Login failed with code: %1").arg(code) : msg);
            }
        } else {
            onAuthFailed(error);
        }
    });
}

bool LoginWindow::validateInput()
{
    if (m_serverHostEdit->text().trimmed().isEmpty()) {
        showError(tr("Please enter server address"));
        return false;
    }
    if (m_usernameEdit->text().trimmed().isEmpty()) {
        showError(tr("Please enter username"));
        return false;
    }
    if (m_passwordEdit->text().isEmpty()) {
        showError(tr("Please enter password"));
        return false;
    }
    return true;
}

void LoginWindow::showError(const QString& message)
{
    m_errorLabel->setText(message);
    m_errorLabel->setVisible(true);
}

void LoginWindow::clearError()
{
    m_errorLabel->setVisible(false);
}

void LoginWindow::setLoading(bool loading)
{
    m_loading = loading;
    m_loginButton->setEnabled(!loading);
    m_usernameEdit->setEnabled(!loading);
    m_passwordEdit->setEnabled(!loading);
    m_serverHostEdit->setEnabled(!loading);
    m_serverPortSpin->setEnabled(!loading);
    m_httpsCheck->setEnabled(!loading);

    if (loading) {
        m_loginButton->setText(tr("Logging in..."));
    } else {
        m_loginButton->setText(tr("Login"));
    }
}

void LoginWindow::onUsernameChanged(const QString& text)
{
    Q_UNUSED(text)
    clearError();
}

void LoginWindow::onPasswordChanged(const QString& text)
{
    Q_UNUSED(text)
    clearError();
}

void LoginWindow::onAuthSuccess(const QJsonObject& data)
{
    setLoading(false);

    QJsonObject innerData = data["data"].toObject();
    QString accessToken = innerData["access_token"].toString();
    QString refreshToken = innerData["refresh_token"].toString();
    QJsonObject userInfo = innerData["user"].toObject();

    SessionManager::instance()->setTokens(accessToken, refreshToken);
    SessionManager::instance()->setUserInfo(userInfo);

    if (m_animationBg) {
        m_animationBg->stopAnimation();
    }

    LOG_INFO("LoginWindow", QString("Login successful for user: %1")
              .arg(userInfo["username"].toString()));

    emit loginSuccess(userInfo);
}

void LoginWindow::onAuthFailed(const QString& error)
{
    setLoading(false);
    showError(error);
}

void LoginWindow::onOpenServerConfig()
{
    ServerConfigDialog dialog(this);
    if (dialog.exec() == QDialog::Accepted) {
        loadServerConfig();
        m_serverHostEdit->setText(ServerConfig::instance()->serverHost());
        m_serverPortSpin->setValue(ServerConfig::instance()->serverPort());
        m_httpsCheck->setChecked(ServerConfig::instance()->useHttps());
    }
}

}
