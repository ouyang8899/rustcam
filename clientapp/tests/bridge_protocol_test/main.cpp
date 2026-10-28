// Unit test for the C++ <-> JS bridge payload contract.
#include "common/CefBridgeProtocol.h"

#include <QCoreApplication>
#include <QJsonObject>
#include <QTextStream>

using namespace rustcam::cefbridge;

static int g_failures = 0;

static void check(bool condition, const QString& what)
{
    QTextStream out(stdout);
    if (condition) {
        out << "PASS " << what << "\n";
    } else {
        out << "FAIL " << what << "\n";
        ++g_failures;
    }
}

int main(int argc, char* argv[])
{
    QCoreApplication app(argc, argv);
    QTextStream out(stdout);

    check(QString::fromLatin1(kNativeMessageName) == QStringLiteral("rustcam.native"),
          "message name is rustcam.native");
    check(QString::fromLatin1(kNativeFunctionName) == QStringLiteral("rustcamNative"),
          "function name is rustcamNative");

    QJsonObject parsed;

    check(parseBridgeMessage(QStringLiteral("{\"type\":\"mapReady\"}"), &parsed),
          "valid payload parses");
    check(parsed.value(QStringLiteral("type")).toString() == QStringLiteral("mapReady"),
          "type field is readable");

    check(parseBridgeMessage(QStringLiteral("{\"type\":\"markerClicked\",\"id\":\"dev-1\"}"), &parsed),
          "marker payload parses");
    check(parsed.value(QStringLiteral("id")).toString() == QStringLiteral("dev-1"),
          "id field is readable");

    check(!parseBridgeMessage(QStringLiteral("not json"), &parsed),
          "malformed payload is rejected");
    check(!parseBridgeMessage(QStringLiteral("[1,2,3]"), &parsed),
          "non-object payload is rejected");
    check(!parseBridgeMessage(QStringLiteral("{\"id\":\"x\"}"), &parsed),
          "payload without type is rejected");
    check(!parseBridgeMessage(QString(), &parsed),
          "empty payload is rejected");

    check(parseBridgeMessage(QStringLiteral("{\"type\":\"mapReady\"}"), nullptr),
          "null out pointer is tolerated");

    out << (g_failures == 0 ? "ALL TESTS PASSED\n" : "TESTS FAILED\n");
    return g_failures == 0 ? 0 : 1;
}
