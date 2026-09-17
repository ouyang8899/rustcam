# Qt Version Compatibility Macros
#pragma once

#include <QtGlobal>

#if QT_VERSION >= QT_VERSION_CHECK(6, 0, 0)
    #define RUSTCAM_QT6
    #define RUSTCAM_QT_MAJOR_VERSION 6
#else
    #define RUSTCAM_QT5
    #define RUSTCAM_QT_MAJOR_VERSION 5
#endif

#if QT_VERSION >= QT_VERSION_CHECK(6, 4, 0)
    #define RUSTCAM_STRINGVIEW QStringView
#else
    #define RUSTCAM_STRINGVIEW QString
#endif

#if QT_VERSION >= QT_VERSION_CHECK(6, 4, 0)
    #define RUSTCAM_HAS_QHTTPSERVER 1
#else
    #define RUSTCAM_HAS_QHTTPSERVER 0
#endif

#if QT_VERSION >= QT_VERSION_CHECK(6, 6, 0)
    #define RUSTCAM_HAS_CANCEL_ON_UNWIND 1
#endif

#define RUSTCAM_FROM_UTF8(s) QString::fromUtf8(s)
#define RUSTCAM_TO_UTF8(s) s.toUtf8()

#if QT_VERSION >= QT_VERSION_CHECK(6, 0, 0)
    #define RUSTCAM_PLUGIN_METADATA_IID "com.rustcam.ClientPlugin/2.0"
#else
    #define RUSTCAM_PLUGIN_METADATA_IID "com.rustcam.ClientPlugin/1.0"
#endif

#define RUSTCAM_CONCAT(a, b) a##b
#define RUSTCAM_STRINGIFY(x) #x

#define RUSTCAM_DECLARE_METATYPE(Type) \
    Q_DECLARE_METATYPE(Type) \
    inline const char* getTypeName(Type*) { return #Type; }

#if defined(RUSTCAM_QT6)
    #include <QStringConverter>
    #define RUSTCAM_FROM_Latin1(s) QString::fromLatin1(s)
    #define RUSTCAM_FROM_ASCII(s) QString::fromLatin1(s)
#else
    #define RUSTCAM_FROM_Latin1(s) QString::fromLatin1(s)
    #define RUSTCAM_FROM_ASCII(s) QString::fromAscii(s)
#endif
