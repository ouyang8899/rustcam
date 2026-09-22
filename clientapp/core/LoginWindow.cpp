#include "LoginWindow.h"
#include "ServerApi.h"
#include "ServerConfig.h"
#include "ServerConfigDialog.h"
#include "SessionManager.h"
#include "Logger.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QGridLayout>
#include <QStackedLayout>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QCheckBox>
#include <QSpacerItem>
#include <QPropertyAnimation>
#include <QPainter>
#include <QPainterPath>
#include <QtMath>
#include <QSpinBox>
#include <cmath>

namespace rustcam {

class AnimatedBackground : public QWidget {
    Q_OBJECT
public:
    AnimatedBackground(QWidget* parent = nullptr) : QWidget(parent) {
        setAttribute(Qt::WA_StaticContents);
        m_animation = new QPropertyAnimation(this, "phase", this);
        m_animation->setStartValue(0.0);
        m_animation->setEndValue(1.0);
        m_animation->setDuration(8000);
        m_animation->setLoopCount(-1);
        m_animation->start();
    }

    qreal phase() const { return m_phase; }
    void setPhase(qreal phase) {
        m_phase = phase;
        update();
    }

protected:
    void paintEvent(QPaintEvent* event) override {
        Q_UNUSED(event)
        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing);

        QLinearGradient gradient(0, 0, width(), height());
        gradient.setColorAt(0, QColor(30, 30, 30));
        gradient.setColorAt(0.5, QColor(26, 26, 46));
        gradient.setColorAt(1, QColor(22, 33, 62));
        painter.fillRect(rect(), gradient);

        painter.setPen(QPen(QColor(0, 120, 212, 15), 1));
        int gridSize = 60;
        for (int x = 0; x < width(); x += gridSize) {
            painter.drawLine(x, 0, x, height());
        }
        for (int y = 0; y < height(); y += gridSize) {
            painter.drawLine(0, y, width(), y);
        }

        QPainterPath path;
        int waveHeight = 30;
        int baseY = height() * 0.6;
        path.moveTo(0, baseY);

        for (int x = 0; x <= width(); x += 5) {
            qreal y1 = baseY + qSin((x + m_phase * 200) * 0.02) * waveHeight;
            qreal y2 = baseY + qSin((x + 50 + m_phase * 200) * 0.025) * waveHeight * 0.6;
            path.quadTo(x, y1, x + 2, y2);
        }

        path.lineTo(width(), height());
        path.lineTo(0, height());
        path.closeSubpath();

        QLinearGradient waveGradient(0, baseY - waveHeight, 0, baseY + waveHeight);
        waveGradient.setColorAt(0, QColor(0, 120, 212, 40));
        waveGradient.setColorAt(1, QColor(0, 120, 212, 5));
        painter.fillPath(path, waveGradient);

        for (int i = 0; i < 8; ++i) {
            qreal x = width() * (0.1 + i * 0.12);
            qreal y = height() * 0.35 + qSin(m_phase * 3.14159 * 2 + i) * 20;
            painter.setPen(QPen(QColor(0, 120, 212, 60), 2));
            painter.drawEllipse(QPointF(x, y), 4, 4);

            QPainterPath linePath;
            linePath.moveTo(x, y + 4);
            qreal lineLen = 30 + qSin(m_phase * 3.14159 * 2 + i * 0.5) * 10;
            linePath.lineTo(x, y + lineLen);
            painter.setPen(QPen(QColor(0, 120, 212, 30), 1));
            painter.drawPath(linePath);
        }

        painter.setPen(QPen(QColor(0, 120, 212, 10), 1));
        for (int i = 0; i < 20; ++i) {
            qreal x1 = std::fmod((i * 73 + m_phase * 100), (qreal)width());
            qreal y1 = std::fmod((i * 97 + m_phase * 50), (qreal)height());
            qreal angle = qDegreesToRadians((qreal)((i * 37) % 360));
            qreal len = 20 + (i * 7) % 30;
            painter.drawLine(QPointF(x1, y1),
                           QPointF(x1 + qCos(angle) * len, y1 + qSin(angle) * len));
        }
    }

private:
    qreal m_phase;
    QPropertyAnimation* m_animation;
};

LoginWindow::LoginWindow(QWidget* parent)
    : QWidget(parent)
    , m_loading(false)
{
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

    QStackedLayout* leftStack = new QStackedLayout(leftPanel);
    leftStack->setContentsMargins(0, 0, 0, 0);
    leftStack->setStackingMode(QStackedLayout::StackAll);

    AnimatedBackground* bg = new AnimatedBackground(leftPanel);
    bg->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
    leftStack->addWidget(bg);

    QWidget* textOverlay = new QWidget(leftPanel);
    textOverlay->setAttribute(Qt::WA_TransparentForMouseEvents);
    QVBoxLayout* textLayout = new QVBoxLayout(textOverlay);
    textLayout->setContentsMargins(50, 50, 50, 50);

    QLabel* titleLabel = new QLabel(textOverlay);
    titleLabel->setText("rustcam");
    titleLabel->setStyleSheet(R"(
        QLabel {
            color: #0078D4;
            font-size: 42px;
            font-weight: bold;
            font-family: 'Segoe UI', 'Microsoft YaHei', sans-serif;
            letter-spacing: 4px;
        }
    )");

    QLabel* subtitleLabel = new QLabel(textOverlay);
    subtitleLabel->setText("Intelligent Video Surveillance System");
    subtitleLabel->setStyleSheet(R"(
        QLabel {
            color: rgba(255, 255, 255, 0.7);
            font-size: 14px;
            font-family: 'Segoe UI', 'Microsoft YaHei', sans-serif;
            letter-spacing: 1px;
        }
    )");

    QLabel* versionLabel = new QLabel(textOverlay);
    versionLabel->setText("v1.0.0");
    versionLabel->setStyleSheet(R"(
        QLabel {
            color: rgba(255, 255, 255, 0.3);
            font-size: 12px;
            font-family: 'Segoe UI', sans-serif;
        }
    )");

    textLayout->addWidget(titleLabel);
    textLayout->addWidget(subtitleLabel);
    textLayout->addWidget(versionLabel);
    textLayout->addStretch();

    leftStack->addWidget(textOverlay);
    leftStack->setCurrentIndex(1);

    mainLayout->addWidget(leftPanel, 1);

    QWidget* rightPanel = new QWidget(this);
    rightPanel->setStyleSheet(R"(
        QWidget {
            background-color: #1e1e1e;
        }
    )");
    QVBoxLayout* rightLayout = new QVBoxLayout(rightPanel);
    rightLayout->setContentsMargins(60, 40, 60, 40);

    QSpacerItem* topSpacer = new QSpacerItem(20, 40, QSizePolicy::Minimum, QSizePolicy::Fixed);
    rightLayout->addSpacerItem(topSpacer);

    QLabel* welcomeLabel = new QLabel(rightPanel);
    welcomeLabel->setText("Welcome");
    welcomeLabel->setStyleSheet(R"(
        QLabel {
            color: #e0e0e0;
            font-size: 28px;
            font-weight: bold;
            font-family: 'Microsoft YaHei', 'Segoe UI', sans-serif;
        }
    )");
    rightLayout->addWidget(welcomeLabel);

    QLabel* descLabel = new QLabel(rightPanel);
    descLabel->setText("Please enter your credentials to login");
    descLabel->setStyleSheet(R"(
        QLabel {
            color: #9e9e9e;
            font-size: 14px;
            font-family: 'Microsoft YaHei', sans-serif;
            margin-bottom: 20px;
        }
    )");
    rightLayout->addWidget(descLabel);

    QGridLayout* formLayout = new QGridLayout();
    formLayout->setSpacing(12);

    QLabel* serverLabel = new QLabel(rightPanel);
    serverLabel->setText("服务器地址");
    serverLabel->setStyleSheet(R"(
        QLabel {
            color: #9e9e9e;
            font-size: 12px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
    )");
    formLayout->addWidget(serverLabel, 0, 0, 1, 2);

    m_serverHostEdit = new QLineEdit(rightPanel);
    m_serverHostEdit->setPlaceholderText("服务器 IP 或域名");
    m_serverHostEdit->setMinimumHeight(40);
    m_serverHostEdit->setText(ServerConfig::instance()->serverHost());
    m_serverHostEdit->setStyleSheet(R"(
        QLineEdit {
            background-color: #2b2b2b;
            border: 1px solid #3e3e3e;
            border-radius: 6px;
            padding: 0 12px;
            color: #e0e0e0;
            font-size: 14px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
        QLineEdit:focus {
            border-color: #0078D4;
            background-color: #252525;
        }
        QLineEdit:disabled {
            background-color: #1a1a1a;
            color: #6e6e6e;
        }
    )");
    formLayout->addWidget(m_serverHostEdit, 1, 0, 1, 1);

    m_serverPortSpin = new QSpinBox(rightPanel);
    m_serverPortSpin->setMinimum(1);
    m_serverPortSpin->setMaximum(65535);
    m_serverPortSpin->setValue(ServerConfig::instance()->serverPort());
    m_serverPortSpin->setMinimumHeight(40);
    m_serverPortSpin->setStyleSheet(R"(
        QSpinBox {
            background-color: #2b2b2b;
            border: 1px solid #3e3e3e;
            border-radius: 6px;
            padding: 0 12px;
            color: #e0e0e0;
            font-size: 14px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
        QSpinBox:focus {
            border-color: #0078D4;
            background-color: #252525;
        }
        QSpinBox::up-button, QSpinBox::down-button {
            background-color: transparent;
            border: none;
            width: 20px;
        }
        QSpinBox::up-arrow {
            image: none;
            border-left: 4px solid transparent;
            border-right: 4px solid transparent;
            border-bottom: 6px solid #9e9e9e;
        }
        QSpinBox::down-arrow {
            image: none;
            border-left: 4px solid transparent;
            border-right: 4px solid transparent;
            border-top: 6px solid #9e9e9e;
        }
    )");
    formLayout->addWidget(m_serverPortSpin, 1, 1, 1, 1);

    m_httpsCheck = new QCheckBox(rightPanel);
    m_httpsCheck->setText("使用 HTTPS");
    m_httpsCheck->setChecked(ServerConfig::instance()->useHttps());
    m_httpsCheck->setStyleSheet(R"(
        QCheckBox {
            color: #9e9e9e;
            font-size: 13px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
        QCheckBox::indicator {
            width: 18px;
            height: 18px;
            border-radius: 4px;
            border: 1px solid #3e3e3e;
            background-color: #2b2b2b;
        }
        QCheckBox::indicator:checked {
            background-color: #0078D4;
            border-color: #0078D4;
        }
        QCheckBox:hover {
            color: #e0e0e0;
        }
    )");
    formLayout->addWidget(m_httpsCheck, 2, 0, 1, 2);

    QLabel* userLabel = new QLabel(rightPanel);
    userLabel->setText("用户名");
    userLabel->setStyleSheet(R"(
        QLabel {
            color: #9e9e9e;
            font-size: 12px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
    )");
    formLayout->addWidget(userLabel, 3, 0, 1, 2);

    m_usernameEdit = new QLineEdit(rightPanel);
    m_usernameEdit->setPlaceholderText("请输入用户名");
    m_usernameEdit->setMinimumHeight(40);
    m_usernameEdit->setStyleSheet(R"(
        QLineEdit {
            background-color: #2b2b2b;
            border: 1px solid #3e3e3e;
            border-radius: 6px;
            padding: 0 12px;
            color: #e0e0e0;
            font-size: 14px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
        QLineEdit:focus {
            border-color: #0078D4;
            background-color: #252525;
        }
        QLineEdit::placeholder {
            color: #6e6e6e;
        }
    )");
    formLayout->addWidget(m_usernameEdit, 4, 0, 1, 2);

    QLabel* pwdLabel = new QLabel(rightPanel);
    pwdLabel->setText("密码");
    pwdLabel->setStyleSheet(R"(
        QLabel {
            color: #9e9e9e;
            font-size: 12px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
    )");
    formLayout->addWidget(pwdLabel, 5, 0, 1, 2);

    m_passwordEdit = new QLineEdit(rightPanel);
    m_passwordEdit->setPlaceholderText("请输入密码");
    m_passwordEdit->setEchoMode(QLineEdit::Password);
    m_passwordEdit->setMinimumHeight(40);
    m_passwordEdit->setStyleSheet(R"(
        QLineEdit {
            background-color: #2b2b2b;
            border: 1px solid #3e3e3e;
            border-radius: 6px;
            padding: 0 12px;
            color: #e0e0e0;
            font-size: 14px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
        QLineEdit:focus {
            border-color: #0078D4;
            background-color: #252525;
        }
        QLineEdit::placeholder {
            color: #6e6e6e;
        }
    )");
    formLayout->addWidget(m_passwordEdit, 6, 0, 1, 2);

    rightLayout->addLayout(formLayout);

    QHBoxLayout* optionsLayout = new QHBoxLayout();
    optionsLayout->setSpacing(16);

    m_rememberCheck = new QCheckBox(rightPanel);
    m_rememberCheck->setText("记住密码");
    m_rememberCheck->setChecked(true);
    m_rememberCheck->setStyleSheet(R"(
        QCheckBox {
            color: #9e9e9e;
            font-size: 13px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
        QCheckBox::indicator {
            width: 18px;
            height: 18px;
            border-radius: 4px;
            border: 1px solid #3e3e3e;
            background-color: #2b2b2b;
        }
        QCheckBox::indicator:checked {
            background-color: #0078D4;
            border-color: #0078D4;
        }
        QCheckBox:hover {
            color: #e0e0e0;
        }
    )");
    optionsLayout->addWidget(m_rememberCheck);

    QLabel* forgotLabel = new QLabel(rightPanel);
    forgotLabel->setText("<a href='#' style='color: #0078D4; text-decoration: none; font-size: 13px; font-family: \"Microsoft YaHei\", sans-serif;'>忘记密码？</a>");
    forgotLabel->setAlignment(Qt::AlignRight);
    forgotLabel->setOpenExternalLinks(false);
    optionsLayout->addWidget(forgotLabel, 1);

    rightLayout->addLayout(optionsLayout);

    m_errorLabel = new QLabel(rightPanel);
    m_errorLabel->setStyleSheet(R"(
        QLabel {
            color: #f44336;
            font-size: 13px;
            font-family: 'Microsoft YaHei', sans-serif;
            background-color: rgba(244, 67, 54, 0.1);
            border: 1px solid rgba(244, 67, 54, 0.3);
            border-radius: 4px;
            padding: 8px 12px;
        }
    )");
    m_errorLabel->setVisible(false);
    rightLayout->addWidget(m_errorLabel);

    m_loginButton = new QPushButton(rightPanel);
    m_loginButton->setText("登 录");
    m_loginButton->setMinimumHeight(50);
    m_loginButton->setCursor(Qt::PointingHandCursor);
    m_loginButton->setStyleSheet(R"(
        QPushButton {
            background-color: #0078D4;
            color: white;
            border: none;
            border-radius: 6px;
            font-size: 16px;
            font-weight: bold;
            font-family: 'Microsoft YaHei', 'Segoe UI', sans-serif;
            letter-spacing: 8px;
        }
        QPushButton:hover {
            background-color: #1084d8;
        }
        QPushButton:pressed {
            background-color: #005a9e;
        }
        QPushButton:disabled {
            background-color: #3e3e3e;
            color: #6e6e6e;
        }
    )");
    rightLayout->addWidget(m_loginButton);

    QSpacerItem* bottomSpacer = new QSpacerItem(20, 12, QSizePolicy::Minimum, QSizePolicy::Fixed);
    rightLayout->addSpacerItem(bottomSpacer);

    QHBoxLayout* footerLayout = new QHBoxLayout();
    footerLayout->setSpacing(8);

    m_serverConfigLink = new QLabel(rightPanel);
    m_serverConfigLink->setText("<a href='#' style='color: #0078D4; text-decoration: none; font-size: 12px; font-family: \"Microsoft YaHei\", sans-serif;'>服务器设置</a>");
    m_serverConfigLink->setCursor(Qt::PointingHandCursor);
    footerLayout->addWidget(m_serverConfigLink);

    footerLayout->addStretch();

    QLabel* copyrightLabel = new QLabel(rightPanel);
    copyrightLabel->setText("(c) 2024 rustcam. All rights reserved.");
    copyrightLabel->setStyleSheet(R"(
        QLabel {
            color: #6e6e6e;
            font-size: 12px;
            font-family: 'Segoe UI', sans-serif;
        }
    )");
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
            onAuthSuccess(response);
        } else {
            onAuthFailed(error);
        }
    });
}

bool LoginWindow::validateInput()
{
    if (m_serverHostEdit->text().trimmed().isEmpty()) {
        showError("请输入服务器地址");
        return false;
    }
    if (m_usernameEdit->text().trimmed().isEmpty()) {
        showError("请输入用户名");
        return false;
    }
    if (m_passwordEdit->text().isEmpty()) {
        showError("请输入密码");
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
        m_loginButton->setText("登录中...");
    } else {
        m_loginButton->setText("登 录");
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

    QString accessToken = data["access_token"].toString();
    QString refreshToken = data["refresh_token"].toString();
    QJsonObject userInfo = data["user"].toObject();

    SessionManager::instance()->setTokens(accessToken, refreshToken);
    SessionManager::instance()->setUserInfo(userInfo);

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

#include "LoginWindow.moc"
