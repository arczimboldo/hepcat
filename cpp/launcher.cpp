// Application shell: QApplication, single-instance handling, the tray icon
// and the QML window.
//
// QApplication (rather than QGuiApplication) is needed for the
// org.kde.desktop Qt Quick Controls style, which makes the UI pick up the
// Plasma theme, colours and fonts, and for QSystemTrayIcon.

#include "hepcat/cpp/launcher.h"

#include <QAction>
#include <QApplication>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QIcon>
#include <QLocalServer>
#include <QLocalSocket>
#include <QMenu>
#include <QQmlApplicationEngine>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QPainter>
#include <QSvgRenderer>
#include <QStandardPaths>
#include <QSystemTrayIcon>
#include <QTimer>

#include <functional>
#include <memory>

namespace {

const QString appId = QStringLiteral("io.github.arczimboldo.Hepcat");
const QString instanceName = appId;

// Where the running instance listens. Inside a Flatpak, /tmp may be private
// to each launch, so it lives in the app's runtime folder, which every
// instance of the app shares.
QString instanceSocket()
{
    const QByteArray app = qgetenv("FLATPAK_ID");
    const QByteArray runtime = qgetenv("XDG_RUNTIME_DIR");
    if (app.isEmpty() || runtime.isEmpty()) {
        return instanceName;
    }
    return QStringLiteral("%1/app/%2/instance").arg(QString::fromLocal8Bit(runtime), QString::fromLocal8Bit(app));
}
const QString appIconPath = QStringLiteral(":/qt/qml/org/hepcat/icons/hepcat.svg");
const QString trayIconPath = QStringLiteral(":/qt/qml/org/hepcat/icons/hepcat-symbolic.svg");

QIcon appIcon()
{
    return QIcon::fromTheme(appId, QIcon(appIconPath));
}

// The tray icon is monochrome like Plasma's own; paint it in the panel's
// text colour so it works on both light and dark themes.
QIcon trayIcon()
{
    QFile file(trayIconPath);
    if (!file.open(QIODevice::ReadOnly)) {
        return appIcon();
    }
    const QString colour = QApplication::palette().color(QPalette::WindowText).name();
    QSvgRenderer renderer(file.readAll().replace("#232629", colour.toLatin1()));
    QIcon icon;
    for (int size : {16, 22, 24, 32, 44, 48, 64}) {
        QPixmap pixmap(size, size);
        pixmap.fill(Qt::transparent);
        QPainter painter(&pixmap);
        renderer.render(&painter);
        painter.end();
        icon.addPixmap(pixmap);
    }
    return icon;
}

// Calls a function whenever the application palette changes.
class PaletteWatcher : public QObject
{
public:
    explicit PaletteWatcher(std::function<void()> onChange, QObject *parent)
        : QObject(parent)
        , m_onChange(std::move(onChange))
    {
    }

protected:
    bool eventFilter(QObject *watched, QEvent *event) override
    {
        if (watched == qApp && event->type() == QEvent::ApplicationPaletteChange) {
            m_onChange();
        }
        return false;
    }

private:
    std::function<void()> m_onChange;
};

bool inFlatpak()
{
    return QFile::exists(QStringLiteral("/.flatpak-info"));
}

// Natively: our login item. In a Flatpak the sandbox cannot see the host's
// autostart folder, so this is only a marker of what we asked the portal for.
QString autostartPath()
{
    if (inFlatpak()) {
        return QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation)
            + QStringLiteral("/autostart-requested");
    }
    return QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation)
        + QStringLiteral("/autostart/") + appId + QStringLiteral(".desktop");
}

// Ask the desktop's Background portal to (not) start us at login.
bool requestPortalAutostart(bool enabled)
{
    QDBusMessage call = QDBusMessage::createMethodCall(QStringLiteral("org.freedesktop.portal.Desktop"),
                                                       QStringLiteral("/org/freedesktop/portal/desktop"),
                                                       QStringLiteral("org.freedesktop.portal.Background"),
                                                       QStringLiteral("RequestBackground"));
    const QVariantMap options{
        {QStringLiteral("reason"), QStringLiteral("Keep your audio routing and effects applied")},
        {QStringLiteral("autostart"), enabled},
        {QStringLiteral("commandline"), QStringList{QStringLiteral("hepcat"), QStringLiteral("--minimized")}},
        {QStringLiteral("dbus-activatable"), false},
    };
    call << QString() << options;
    const QDBusMessage reply = QDBusConnection::sessionBus().call(call);
    if (reply.type() == QDBusMessage::ErrorMessage) {
        qWarning("hepcat: background portal refused: %s", qPrintable(reply.errorMessage()));
        return false;
    }
    return true;
}

bool setAutostart(bool enabled)
{
    const QString path = autostartPath();
    if (inFlatpak()) {
        if (!requestPortalAutostart(enabled)) {
            return false;
        }
        if (!enabled) {
            return QFile::remove(path) || !QFile::exists(path);
        }
        QDir().mkpath(QFileInfo(path).absolutePath());
        QFile marker(path);
        return marker.open(QIODevice::WriteOnly);
    }
    if (!enabled) {
        return QFile::remove(path) || !QFile::exists(path);
    }
    QDir().mkpath(QFileInfo(path).absolutePath());
    QFile file(path);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        return false;
    }
    file.write(QStringLiteral("[Desktop Entry]\n"
                              "Type=Application\n"
                              "Name=Hepcat\n"
                              "Comment=Keeps your audio routing applied\n"
                              "Exec=\"%1\" --minimized\n"
                              "Icon=%2\n"
                              "X-KDE-autostart-phase=2\n")
                   .arg(QCoreApplication::applicationFilePath(), appId)
                   .toUtf8());
    return true;
}

// Carry over the login item of earlier builds: when the app was called
// Mixwire, and before it had a Flathub-style app ID.
void migrateAutostart()
{
    if (inFlatpak()) {
        return;
    }
    const QString dir = QFileInfo(autostartPath()).absolutePath();
    for (const auto &name : {QStringLiteral("org.mixwire.Mixwire.desktop"), QStringLiteral("org.hepcat.Hepcat.desktop")}) {
        const QString old = dir + QLatin1Char('/') + name;
        if (QFile::exists(old) && QFile::remove(old)) {
            setAutostart(true);
        }
    }
}

void raise(QQuickWindow *window)
{
    window->show();
    window->raise();
    window->requestActivate();
}

} // namespace

std::int32_t runApp(bool startHidden)
{
    static int argc = 1;
    static char name[] = "hepcat";
    static char *argv[] = {name, nullptr};

    QApplication app(argc, argv);
    QApplication::setApplicationName(QStringLiteral("hepcat"));
    QApplication::setApplicationDisplayName(QStringLiteral("Hepcat"));
    QApplication::setDesktopFileName(appId);
    QApplication::setWindowIcon(appIcon());

    // A second launch just brings the running instance's window forward.
    // (Screenshot runs are throwaway instances and must not touch a real one.)
    const bool screenshot = !qEnvironmentVariableIsEmpty("HEPCAT_SCREENSHOT");
    if (!screenshot) {
        QLocalSocket probe;
        probe.connectToServer(instanceSocket());
        if (probe.waitForConnected(300)) {
            probe.write(startHidden ? "noop" : "show");
            probe.waitForBytesWritten(300);
            return 0;
        }
    }

    if (qEnvironmentVariableIsEmpty("QT_QUICK_CONTROLS_STYLE")) {
        QQuickStyle::setStyle(QStringLiteral("org.kde.desktop"));
    }
    if (QIcon::themeName().isEmpty()) {
        QIcon::setThemeName(QStringLiteral("breeze"));
    }

    QQmlApplicationEngine engine;
    QObject::connect(
        &engine, &QQmlApplicationEngine::objectCreationFailed, &app,
        [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    engine.setInitialProperties({{QStringLiteral("startHidden"), startHidden}});
    engine.loadFromModule("org.hepcat", "Main");
    if (engine.rootObjects().isEmpty()) {
        return 1;
    }
    auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().constFirst());

    QLocalServer server;
    if (!screenshot) {
        migrateAutostart();
        QLocalServer::removeServer(instanceSocket());
        server.listen(instanceSocket());
    }
    QObject::connect(&server, &QLocalServer::newConnection, &app, [&server, window] {
        QLocalSocket *client = server.nextPendingConnection();
        QObject::connect(client, &QLocalSocket::readyRead, client, [client, window] {
            if (client->readAll() == "show") {
                raise(window);
            }
            client->deleteLater();
        });
    });

    // With a tray, closing the window keeps Hepcat (and its routing rules)
    // running in the background.
    std::unique_ptr<QSystemTrayIcon> tray;
    QMenu menu;
    if (QSystemTrayIcon::isSystemTrayAvailable() && !screenshot) {
        QApplication::setQuitOnLastWindowClosed(false);
        tray = std::make_unique<QSystemTrayIcon>(trayIcon());
        QSystemTrayIcon *trayPtr = tray.get();
        app.installEventFilter(new PaletteWatcher([trayPtr] { trayPtr->setIcon(trayIcon()); }, &app));
        tray->setToolTip(QStringLiteral("Hepcat"));

        QAction *show = menu.addAction(QIcon::fromTheme(QStringLiteral("window")), QStringLiteral("Show Hepcat"));
        QObject::connect(show, &QAction::triggered, window, [window] { raise(window); });

        QAction *autostart = menu.addAction(QStringLiteral("Start at Login"));
        autostart->setCheckable(true);
        autostart->setChecked(QFile::exists(autostartPath()));
        QObject::connect(autostart, &QAction::toggled, autostart, [autostart](bool on) {
            if (!setAutostart(on)) {
                const QSignalBlocker block(autostart);
                autostart->setChecked(!on);
            }
        });

        menu.addSeparator();
        QAction *quit = menu.addAction(QIcon::fromTheme(QStringLiteral("application-exit")), QStringLiteral("Quit"));
        QObject::connect(quit, &QAction::triggered, &app, &QCoreApplication::quit);

        tray->setContextMenu(&menu);
        QObject::connect(tray.get(), &QSystemTrayIcon::activated, window, [window](QSystemTrayIcon::ActivationReason reason) {
            if (reason != QSystemTrayIcon::Trigger) {
                return;
            }
            if (window->isVisible() && window->isActive()) {
                window->hide();
            } else {
                raise(window);
            }
        });
        tray->show();
    } else if (startHidden) {
        // Nowhere to hide to: show the window after all.
        raise(window);
    }

    // Development aid: HEPCAT_SCREENSHOT=out.png renders the window to a file
    // and exits (combine with QT_QPA_PLATFORM=offscreen to keep it off screen).
    const QString shot = qEnvironmentVariable("HEPCAT_SCREENSHOT");
    if (!shot.isEmpty()) {
        QTimer::singleShot(2500, &app, [window, shot] {
            window->grabWindow().save(shot);
            QCoreApplication::quit();
        });
    }

    return app.exec();
}
