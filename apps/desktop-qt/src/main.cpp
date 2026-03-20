#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QUrl>

#include "DesktopController.h"
#include "ScatterPlotItem.h"

int main(int argc, char *argv[]) {
    QGuiApplication app(argc, argv);
    QQmlApplicationEngine engine;
    DesktopController controller;

    qmlRegisterType<ScatterPlotItem>("Flowjoish", 1, 0, "ScatterPlotItem");
    engine.rootContext()->setContextProperty("desktopController", &controller);

    const QUrl url = QUrl::fromLocalFile(QStringLiteral(FLOWJOISH_QML_DIR "/Main.qml"));
    QObject::connect(
        &engine,
        &QQmlApplicationEngine::objectCreated,
        &app,
        [url](QObject *object, const QUrl &objectUrl) {
            if (!object && url == objectUrl) {
                QCoreApplication::exit(-1);
            }
        },
        Qt::QueuedConnection);

    engine.load(url);
    return app.exec();
}
