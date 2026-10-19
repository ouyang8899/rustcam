/********************************************************************************
** Form generated from reading UI file 'PTZControlPanel.ui'
**
** Created by: Qt User Interface Compiler version 5.15.2
**
** WARNING! All changes made in this file will be lost when recompiling UI file!
********************************************************************************/

#ifndef UI_PTZCONTROLPANEL_H
#define UI_PTZCONTROLPANEL_H

#include <QtCore/QVariant>
#include <QtWidgets/QApplication>
#include <QtWidgets/QFrame>
#include <QtWidgets/QGridLayout>
#include <QtWidgets/QHBoxLayout>
#include <QtWidgets/QLabel>
#include <QtWidgets/QListView>
#include <QtWidgets/QSpacerItem>
#include <QtWidgets/QToolButton>
#include <QtWidgets/QVBoxLayout>
#include <QtWidgets/QWidget>

QT_BEGIN_NAMESPACE

class Ui_PTZControlPanel
{
public:
    QVBoxLayout *verticalLayout;
    QWidget *m_titleBar;
    QHBoxLayout *horizontalLayout;
    QToolButton *m_toggleBtn;
    QLabel *m_titleLabel;
    QSpacerItem *horizontalSpacer;
    QWidget *m_contentWidget;
    QVBoxLayout *contentLayout;
    QFrame *dpadFrame;
    QGridLayout *dpadLayout;
    QToolButton *btnUpLeft;
    QToolButton *btnUp;
    QToolButton *btnUpRight;
    QToolButton *btnLeft;
    QToolButton *btnCenter;
    QToolButton *btnRight;
    QToolButton *btnDownLeft;
    QToolButton *btnDown;
    QToolButton *btnDownRight;
    QFrame *auxFrame;
    QVBoxLayout *auxLayout;
    QHBoxLayout *auxRow1;
    QSpacerItem *auxSpacer1Left;
    QToolButton *btnZoomIn;
    QToolButton *btnZoomOut;
    QToolButton *btnFocusNear;
    QSpacerItem *auxSpacer1Right;
    QHBoxLayout *auxRow2;
    QSpacerItem *auxSpacer2Left;
    QToolButton *btnFocusFar;
    QToolButton *btnIrisOpen;
    QToolButton *btnIrisClose;
    QSpacerItem *auxSpacer2Right;
    QWidget *presetContainer;
    QVBoxLayout *presetContainerLayout;
    QWidget *presetHeader;
    QHBoxLayout *presetHeaderLayout;
    QToolButton *btnPresetToggle;
    QLabel *presetLabel;
    QSpacerItem *presetHeaderSpacer;
    QToolButton *btnPresetAdd;
    QToolButton *btnPresetRemove;
    QListView *presetListView;

    void setupUi(QWidget *PTZControlPanel)
    {
        if (PTZControlPanel->objectName().isEmpty())
            PTZControlPanel->setObjectName(QString::fromUtf8("PTZControlPanel"));
        PTZControlPanel->resize(200, 400);
        verticalLayout = new QVBoxLayout(PTZControlPanel);
        verticalLayout->setSpacing(0);
        verticalLayout->setObjectName(QString::fromUtf8("verticalLayout"));
        verticalLayout->setContentsMargins(0, 0, 0, 0);
        m_titleBar = new QWidget(PTZControlPanel);
        m_titleBar->setObjectName(QString::fromUtf8("m_titleBar"));
        m_titleBar->setMinimumSize(QSize(0, 32));
        m_titleBar->setMaximumSize(QSize(16777215, 32));
        horizontalLayout = new QHBoxLayout(m_titleBar);
        horizontalLayout->setSpacing(4);
        horizontalLayout->setObjectName(QString::fromUtf8("horizontalLayout"));
        horizontalLayout->setContentsMargins(8, 0, 8, 0);
        m_toggleBtn = new QToolButton(m_titleBar);
        m_toggleBtn->setObjectName(QString::fromUtf8("m_toggleBtn"));
        m_toggleBtn->setMinimumSize(QSize(20, 20));
        m_toggleBtn->setMaximumSize(QSize(20, 20));
        m_toggleBtn->setArrowType(Qt::DownArrow);

        horizontalLayout->addWidget(m_toggleBtn);

        m_titleLabel = new QLabel(m_titleBar);
        m_titleLabel->setObjectName(QString::fromUtf8("m_titleLabel"));

        horizontalLayout->addWidget(m_titleLabel);

        horizontalSpacer = new QSpacerItem(40, 20, QSizePolicy::Expanding, QSizePolicy::Minimum);

        horizontalLayout->addItem(horizontalSpacer);


        verticalLayout->addWidget(m_titleBar);

        m_contentWidget = new QWidget(PTZControlPanel);
        m_contentWidget->setObjectName(QString::fromUtf8("m_contentWidget"));
        contentLayout = new QVBoxLayout(m_contentWidget);
        contentLayout->setSpacing(8);
        contentLayout->setObjectName(QString::fromUtf8("contentLayout"));
        contentLayout->setContentsMargins(8, 8, 8, 8);
        dpadFrame = new QFrame(m_contentWidget);
        dpadFrame->setObjectName(QString::fromUtf8("dpadFrame"));
        dpadFrame->setMinimumSize(QSize(150, 150));
        dpadFrame->setFrameShape(QFrame::Box);
        dpadLayout = new QGridLayout(dpadFrame);
        dpadLayout->setSpacing(2);
        dpadLayout->setObjectName(QString::fromUtf8("dpadLayout"));
        dpadLayout->setContentsMargins(4, 4, 4, 4);
        btnUpLeft = new QToolButton(dpadFrame);
        btnUpLeft->setObjectName(QString::fromUtf8("btnUpLeft"));
        btnUpLeft->setMinimumSize(QSize(40, 40));

        dpadLayout->addWidget(btnUpLeft, 0, 0, 1, 1);

        btnUp = new QToolButton(dpadFrame);
        btnUp->setObjectName(QString::fromUtf8("btnUp"));
        btnUp->setMinimumSize(QSize(40, 40));

        dpadLayout->addWidget(btnUp, 0, 1, 1, 1);

        btnUpRight = new QToolButton(dpadFrame);
        btnUpRight->setObjectName(QString::fromUtf8("btnUpRight"));
        btnUpRight->setMinimumSize(QSize(40, 40));

        dpadLayout->addWidget(btnUpRight, 0, 2, 1, 1);

        btnLeft = new QToolButton(dpadFrame);
        btnLeft->setObjectName(QString::fromUtf8("btnLeft"));
        btnLeft->setMinimumSize(QSize(40, 40));

        dpadLayout->addWidget(btnLeft, 1, 0, 1, 1);

        btnCenter = new QToolButton(dpadFrame);
        btnCenter->setObjectName(QString::fromUtf8("btnCenter"));
        btnCenter->setMinimumSize(QSize(40, 40));

        dpadLayout->addWidget(btnCenter, 1, 1, 1, 1);

        btnRight = new QToolButton(dpadFrame);
        btnRight->setObjectName(QString::fromUtf8("btnRight"));
        btnRight->setMinimumSize(QSize(40, 40));

        dpadLayout->addWidget(btnRight, 1, 2, 1, 1);

        btnDownLeft = new QToolButton(dpadFrame);
        btnDownLeft->setObjectName(QString::fromUtf8("btnDownLeft"));
        btnDownLeft->setMinimumSize(QSize(40, 40));

        dpadLayout->addWidget(btnDownLeft, 2, 0, 1, 1);

        btnDown = new QToolButton(dpadFrame);
        btnDown->setObjectName(QString::fromUtf8("btnDown"));
        btnDown->setMinimumSize(QSize(40, 40));

        dpadLayout->addWidget(btnDown, 2, 1, 1, 1);

        btnDownRight = new QToolButton(dpadFrame);
        btnDownRight->setObjectName(QString::fromUtf8("btnDownRight"));
        btnDownRight->setMinimumSize(QSize(40, 40));

        dpadLayout->addWidget(btnDownRight, 2, 2, 1, 1);


        contentLayout->addWidget(dpadFrame);

        auxFrame = new QFrame(m_contentWidget);
        auxFrame->setObjectName(QString::fromUtf8("auxFrame"));
        auxFrame->setMinimumSize(QSize(0, 80));
        auxFrame->setMaximumSize(QSize(16777215, 80));
        auxFrame->setFrameShape(QFrame::Box);
        auxLayout = new QVBoxLayout(auxFrame);
        auxLayout->setSpacing(4);
        auxLayout->setObjectName(QString::fromUtf8("auxLayout"));
        auxLayout->setContentsMargins(4, 4, 4, 4);
        auxRow1 = new QHBoxLayout();
        auxRow1->setSpacing(4);
        auxRow1->setObjectName(QString::fromUtf8("auxRow1"));
        auxSpacer1Left = new QSpacerItem(20, 20, QSizePolicy::Expanding, QSizePolicy::Minimum);

        auxRow1->addItem(auxSpacer1Left);

        btnZoomIn = new QToolButton(auxFrame);
        btnZoomIn->setObjectName(QString::fromUtf8("btnZoomIn"));
        btnZoomIn->setMinimumSize(QSize(30, 30));

        auxRow1->addWidget(btnZoomIn);

        btnZoomOut = new QToolButton(auxFrame);
        btnZoomOut->setObjectName(QString::fromUtf8("btnZoomOut"));
        btnZoomOut->setMinimumSize(QSize(30, 30));

        auxRow1->addWidget(btnZoomOut);

        btnFocusNear = new QToolButton(auxFrame);
        btnFocusNear->setObjectName(QString::fromUtf8("btnFocusNear"));
        btnFocusNear->setMinimumSize(QSize(30, 30));

        auxRow1->addWidget(btnFocusNear);

        auxSpacer1Right = new QSpacerItem(20, 20, QSizePolicy::Expanding, QSizePolicy::Minimum);

        auxRow1->addItem(auxSpacer1Right);


        auxLayout->addLayout(auxRow1);

        auxRow2 = new QHBoxLayout();
        auxRow2->setSpacing(4);
        auxRow2->setObjectName(QString::fromUtf8("auxRow2"));
        auxSpacer2Left = new QSpacerItem(20, 20, QSizePolicy::Expanding, QSizePolicy::Minimum);

        auxRow2->addItem(auxSpacer2Left);

        btnFocusFar = new QToolButton(auxFrame);
        btnFocusFar->setObjectName(QString::fromUtf8("btnFocusFar"));
        btnFocusFar->setMinimumSize(QSize(30, 30));

        auxRow2->addWidget(btnFocusFar);

        btnIrisOpen = new QToolButton(auxFrame);
        btnIrisOpen->setObjectName(QString::fromUtf8("btnIrisOpen"));
        btnIrisOpen->setMinimumSize(QSize(30, 30));

        auxRow2->addWidget(btnIrisOpen);

        btnIrisClose = new QToolButton(auxFrame);
        btnIrisClose->setObjectName(QString::fromUtf8("btnIrisClose"));
        btnIrisClose->setMinimumSize(QSize(30, 30));

        auxRow2->addWidget(btnIrisClose);

        auxSpacer2Right = new QSpacerItem(20, 20, QSizePolicy::Expanding, QSizePolicy::Minimum);

        auxRow2->addItem(auxSpacer2Right);


        auxLayout->addLayout(auxRow2);


        contentLayout->addWidget(auxFrame);

        presetContainer = new QWidget(m_contentWidget);
        presetContainer->setObjectName(QString::fromUtf8("presetContainer"));
        presetContainerLayout = new QVBoxLayout(presetContainer);
        presetContainerLayout->setSpacing(0);
        presetContainerLayout->setObjectName(QString::fromUtf8("presetContainerLayout"));
        presetContainerLayout->setContentsMargins(0, 0, 0, 0);
        presetHeader = new QWidget(presetContainer);
        presetHeader->setObjectName(QString::fromUtf8("presetHeader"));
        presetHeader->setMinimumSize(QSize(0, 32));
        presetHeader->setMaximumSize(QSize(16777215, 32));
        presetHeaderLayout = new QHBoxLayout(presetHeader);
        presetHeaderLayout->setSpacing(4);
        presetHeaderLayout->setObjectName(QString::fromUtf8("presetHeaderLayout"));
        presetHeaderLayout->setContentsMargins(8, 0, 8, 0);
        btnPresetToggle = new QToolButton(presetHeader);
        btnPresetToggle->setObjectName(QString::fromUtf8("btnPresetToggle"));
        btnPresetToggle->setMinimumSize(QSize(20, 20));
        btnPresetToggle->setMaximumSize(QSize(20, 20));
        btnPresetToggle->setArrowType(Qt::DownArrow);

        presetHeaderLayout->addWidget(btnPresetToggle);

        presetLabel = new QLabel(presetHeader);
        presetLabel->setObjectName(QString::fromUtf8("presetLabel"));

        presetHeaderLayout->addWidget(presetLabel);

        presetHeaderSpacer = new QSpacerItem(40, 20, QSizePolicy::Expanding, QSizePolicy::Minimum);

        presetHeaderLayout->addItem(presetHeaderSpacer);

        btnPresetAdd = new QToolButton(presetHeader);
        btnPresetAdd->setObjectName(QString::fromUtf8("btnPresetAdd"));
        btnPresetAdd->setMinimumSize(QSize(24, 24));
        btnPresetAdd->setMaximumSize(QSize(24, 24));

        presetHeaderLayout->addWidget(btnPresetAdd);

        btnPresetRemove = new QToolButton(presetHeader);
        btnPresetRemove->setObjectName(QString::fromUtf8("btnPresetRemove"));
        btnPresetRemove->setMinimumSize(QSize(24, 24));
        btnPresetRemove->setMaximumSize(QSize(24, 24));

        presetHeaderLayout->addWidget(btnPresetRemove);


        presetContainerLayout->addWidget(presetHeader);

        presetListView = new QListView(presetContainer);
        presetListView->setObjectName(QString::fromUtf8("presetListView"));
        presetListView->setMinimumSize(QSize(0, 100));
        presetListView->setMaximumSize(QSize(16777215, 150));

        presetContainerLayout->addWidget(presetListView);


        contentLayout->addWidget(presetContainer);


        verticalLayout->addWidget(m_contentWidget);


        retranslateUi(PTZControlPanel);

        QMetaObject::connectSlotsByName(PTZControlPanel);
    } // setupUi

    void retranslateUi(QWidget *PTZControlPanel)
    {
        PTZControlPanel->setWindowTitle(QCoreApplication::translate("PTZControlPanel", "PTZ Control Panel", nullptr));
        m_toggleBtn->setText(QCoreApplication::translate("PTZControlPanel", "...", nullptr));
        m_titleLabel->setText(QCoreApplication::translate("PTZControlPanel", "PTZ Control", nullptr));
        btnUpLeft->setText(QString());
        btnUp->setText(QString());
        btnUpRight->setText(QString());
        btnLeft->setText(QString());
        btnCenter->setText(QString());
        btnRight->setText(QString());
        btnDownLeft->setText(QString());
        btnDown->setText(QString());
        btnDownRight->setText(QString());
        btnZoomIn->setText(QString());
        btnZoomOut->setText(QString());
        btnFocusNear->setText(QString());
        btnFocusFar->setText(QString());
        btnIrisOpen->setText(QString());
        btnIrisClose->setText(QString());
        presetLabel->setText(QCoreApplication::translate("PTZControlPanel", "Presets", nullptr));
        btnPresetAdd->setText(QCoreApplication::translate("PTZControlPanel", "+", nullptr));
        btnPresetRemove->setText(QCoreApplication::translate("PTZControlPanel", "-", nullptr));
    } // retranslateUi

};

namespace Ui {
    class PTZControlPanel: public Ui_PTZControlPanel {};
} // namespace Ui

QT_END_NAMESPACE

#endif // UI_PTZCONTROLPANEL_H
