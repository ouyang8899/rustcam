#include "ServerConfigDialog.h"
#include "ServerConfig.h"
#include "ServerApi.h"
#include "Logger.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QFormLayout>
#include <QLabel>
#include <QLineEdit>
#include <QSpinBox>
#include <QCheckBox>
#include <QPushButton>
#include <QMessageBox>
#include <QIntValidator>
#include <QTimer>

namespace rustcam {

ServerConfigDialog::ServerConfigDialog(QWidget* parent)
    : QDialog(parent)
{
    setWindowTitle("服务器配置");
    setWindowFlags(Qt::Dialog | Qt::WindowTitleHint | Qt::WindowCloseButtonHint);
    setMinimumSize(450, 280);
    setStyleSheet(R"(
        QDialog {
            background-color: #1e1e1e;
        }
    )");

    setupUi();
    loadCurrentConfig();
}

ServerConfigDialog::~ServerConfigDialog()
{
}

void ServerConfigDialog::setupUi()
{
    QVBoxLayout* mainLayout = new QVBoxLayout(this);
    mainLayout->setContentsMargins(24, 20, 24, 20);
    mainLayout->setSpacing(16);

    QLabel* titleLabel = new QLabel("服务器设置", this);
    titleLabel->setStyleSheet(R"(
        QLabel {
            color: #e0e0e0;
            font-size: 18px;
            font-weight: bold;
            font-family: 'Microsoft YaHei', 'Segoe UI', sans-serif;
        }
    )");
    mainLayout->addWidget(titleLabel);

    QLabel* descLabel = new QLabel("配置后端服务器的连接信息", this);
    descLabel->setStyleSheet(R"(
        QLabel {
            color: #9e9e9e;
            font-size: 13px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
    )");
    mainLayout->addWidget(descLabel);

    QFrame* formFrame = new QFrame(this);
    formFrame->setStyleSheet(R"(
        QFrame {
            background-color: #2b2b2b;
            border: 1px solid #3e3e3e;
            border-radius: 8px;
        }
    )");
    QFormLayout* formLayout = new QFormLayout(formFrame);
    formLayout->setContentsMargins(16, 16, 16, 16);
    formLayout->setSpacing(12);
    formLayout->setLabelAlignment(Qt::AlignRight);

    QLabel* hostLabel = new QLabel("服务器地址:", this);
    hostLabel->setStyleSheet(R"(
        QLabel {
            color: #e0e0e0;
            font-size: 14px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
    )");
    m_serverHostEdit = new QLineEdit(this);
    m_serverHostEdit->setPlaceholderText("输入 IP 地址或域名");
    m_serverHostEdit->setMinimumHeight(40);
    m_serverHostEdit->setStyleSheet(R"(
        QLineEdit {
            background-color: #1a1a1a;
            border: 1px solid #3e3e3e;
            border-radius: 6px;
            padding: 0 12px;
            color: #e0e0e0;
            font-size: 14px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
        QLineEdit:focus {
            border-color: #0078D4;
        }
        QLineEdit:disabled {
            background-color: #151515;
            color: #6e6e6e;
        }
    )");
    formLayout->addRow(hostLabel, m_serverHostEdit);

    QLabel* portLabel = new QLabel("端口:", this);
    portLabel->setStyleSheet(R"(
        QLabel {
            color: #e0e0e0;
            font-size: 14px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
    )");
    m_serverPortSpin = new QSpinBox(this);
    m_serverPortSpin->setMinimum(1);
    m_serverPortSpin->setMaximum(65535);
    m_serverPortSpin->setMinimumHeight(40);
    m_serverPortSpin->setStyleSheet(R"(
        QSpinBox {
            background-color: #1a1a1a;
            border: 1px solid #3e3e3e;
            border-radius: 6px;
            padding: 0 12px;
            color: #e0e0e0;
            font-size: 14px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
        QSpinBox:focus {
            border-color: #0078D4;
        }
        QSpinBox::up-button, QSpinBox::down-button {
            background-color: transparent;
            border: none;
            width: 24px;
        }
        QSpinBox::up-arrow {
            image: none;
            border-left: 5px solid transparent;
            border-right: 5px solid transparent;
            border-bottom: 6px solid #9e9e9e;
        }
        QSpinBox::down-arrow {
            image: none;
            border-left: 5px solid transparent;
            border-right: 5px solid transparent;
            border-top: 6px solid #9e9e9e;
        }
    )");
    formLayout->addRow(portLabel, m_serverPortSpin);

    QLabel* httpsLabel = new QLabel("加密连接:", this);
    httpsLabel->setStyleSheet(R"(
        QLabel {
            color: #e0e0e0;
            font-size: 14px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
    )");
    m_httpsCheck = new QCheckBox("使用 HTTPS (SSL)", this);
    m_httpsCheck->setStyleSheet(R"(
        QCheckBox {
            color: #e0e0e0;
            font-size: 14px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
        QCheckBox::indicator {
            width: 20px;
            height: 20px;
            border-radius: 4px;
            border: 1px solid #3e3e3e;
            background-color: #1a1a1a;
        }
        QCheckBox::indicator:checked {
            background-color: #0078D4;
            border-color: #0078D4;
        }
    )");
    formLayout->addRow(httpsLabel, m_httpsCheck);

    mainLayout->addWidget(formFrame);

    m_statusLabel = new QLabel(this);
    m_statusLabel->setStyleSheet(R"(
        QLabel {
            color: #9e9e9e;
            font-size: 12px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
    )");
    mainLayout->addWidget(m_statusLabel);

    mainLayout->addStretch();

    QHBoxLayout* buttonLayout = new QHBoxLayout();
    buttonLayout->setSpacing(12);

    m_testButton = new QPushButton("测试连接", this);
    m_testButton->setMinimumHeight(40);
    m_testButton->setCursor(Qt::PointingHandCursor);
    m_testButton->setStyleSheet(R"(
        QPushButton {
            background-color: #3e3e3e;
            color: #e0e0e0;
            border: none;
            border-radius: 6px;
            font-size: 14px;
            font-family: 'Microsoft YaHei', sans-serif;
            min-width: 100px;
        }
        QPushButton:hover {
            background-color: #4e4e4e;
        }
        QPushButton:pressed {
            background-color: #2e2e2e;
        }
        QPushButton:disabled {
            background-color: #2a2a2a;
            color: #6e6e6e;
        }
    )");
    buttonLayout->addWidget(m_testButton);

    buttonLayout->addStretch();

    m_cancelButton = new QPushButton("取消", this);
    m_cancelButton->setMinimumHeight(40);
    m_cancelButton->setCursor(Qt::PointingHandCursor);
    m_cancelButton->setStyleSheet(R"(
        QPushButton {
            background-color: #3e3e3e;
            color: #e0e0e0;
            border: none;
            border-radius: 6px;
            font-size: 14px;
            font-family: 'Microsoft YaHei', sans-serif;
            min-width: 80px;
        }
        QPushButton:hover {
            background-color: #4e4e4e;
        }
        QPushButton:pressed {
            background-color: #2e2e2e;
        }
    )");
    buttonLayout->addWidget(m_cancelButton);

    m_saveButton = new QPushButton("保存", this);
    m_saveButton->setMinimumHeight(40);
    m_saveButton->setCursor(Qt::PointingHandCursor);
    m_saveButton->setDefault(true);
    m_saveButton->setStyleSheet(R"(
        QPushButton {
            background-color: #0078D4;
            color: white;
            border: none;
            border-radius: 6px;
            font-size: 14px;
            font-weight: bold;
            font-family: 'Microsoft YaHei', sans-serif;
            min-width: 80px;
        }
        QPushButton:hover {
            background-color: #1084d8;
        }
        QPushButton:pressed {
            background-color: #005a9e;
        }
    )");
    buttonLayout->addWidget(m_saveButton);

    mainLayout->addLayout(buttonLayout);

    connect(m_saveButton, &QPushButton::clicked, this, &ServerConfigDialog::onSaveClicked);
    connect(m_testButton, &QPushButton::clicked, this, &ServerConfigDialog::onTestClicked);
    connect(m_cancelButton, &QPushButton::clicked, this, &QDialog::reject);
}

void ServerConfigDialog::loadCurrentConfig()
{
    ServerConfig* config = ServerConfig::instance();
    config->load();

    m_serverHostEdit->setText(config->serverHost());
    m_serverPortSpin->setValue(config->serverPort());
    m_httpsCheck->setChecked(config->useHttps());

    m_statusLabel->setText(QString("当前服务器: %1").arg(config->serverUrl()));
}

void ServerConfigDialog::onSaveClicked()
{
    QString host = m_serverHostEdit->text().trimmed();
    if (host.isEmpty()) {
        QMessageBox::warning(this, "警告", "请输入服务器地址");
        return;
    }

    ServerConfig* config = ServerConfig::instance();
    config->setServerHost(host);
    config->setServerPort(m_serverPortSpin->value());
    config->setUseHttps(m_httpsCheck->isChecked());

    LOG_INFO("ServerConfigDialog", QString("Server config saved: %1").arg(config->serverUrl()));

    m_statusLabel->setText(QString("已保存: %1").arg(config->serverUrl()));
    m_statusLabel->setStyleSheet(R"(
        QLabel {
            color: #4caf50;
            font-size: 12px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
    )");

    QTimer::singleShot(500, this, [this]() {
        accept();
    });
}

void ServerConfigDialog::onTestClicked()
{
    QString host = m_serverHostEdit->text().trimmed();
    if (host.isEmpty()) {
        QMessageBox::warning(this, "警告", "请输入服务器地址");
        return;
    }

    m_testButton->setEnabled(false);
    m_testButton->setText("测试中...");
    m_statusLabel->setText("正在连接...");
    m_statusLabel->setStyleSheet(R"(
        QLabel {
            color: #9e9e9e;
            font-size: 12px;
            font-family: 'Microsoft YaHei', sans-serif;
        }
    )");

    ServerConfig* config = ServerConfig::instance();
    config->setServerHost(host);
    config->setServerPort(m_serverPortSpin->value());
    config->setUseHttps(m_httpsCheck->isChecked());

    ServerApi::instance()->get("/api/v1/auth/login", [this](bool success, const QJsonObject&, const QString& error) {
        m_testButton->setEnabled(true);
        m_testButton->setText("测试连接");

        if (success) {
            m_statusLabel->setText("连接成功!");
            m_statusLabel->setStyleSheet(R"(
                QLabel {
                    color: #4caf50;
                    font-size: 12px;
                    font-family: 'Microsoft YaHei', sans-serif;
                }
            )");
        } else {
            m_statusLabel->setText(QString("连接失败: %1").arg(error));
            m_statusLabel->setStyleSheet(R"(
                QLabel {
                    color: #f44336;
                    font-size: 12px;
                    font-family: 'Microsoft YaHei', sans-serif;
                }
            )");
        }
    });
}

}
