#include "hepcat/cpp/shortcuts.h"

#include <KGlobalAccel>
#include <QAction>
#include <QCoreApplication>
#include <QDBusConnection>
#include <QDBusConnectionInterface>
#include <QHash>
#include <QKeySequence>
#include <QPointer>

namespace {

QPointer<QObject> receiver;

// Showing a saved state (HEPCAT_FAKE_STATE) must not take over real keys.
bool fake()
{
    return !qEnvironmentVariableIsEmpty("HEPCAT_FAKE_STATE");
}
QHash<QString, QAction *> actions;

const QString component = QStringLiteral("hepcat");

void notify(const char *method, const QVariant &a, const QVariant &b)
{
    if (receiver) {
        QMetaObject::invokeMethod(receiver, method, Q_ARG(QVariant, a), Q_ARG(QVariant, b));
    }
}

QKeySequence parse(const QString &keys)
{
    return QKeySequence::fromString(keys, QKeySequence::PortableText);
}

} // namespace

void setShortcutReceiver(QObject *root)
{
    receiver = root;
    static bool connected = false;
    if (connected) {
        return;
    }
    connected = true;
    // Key releases, for push-to-talk style shortcuts.
    QObject::connect(KGlobalAccel::self(), &KGlobalAccel::globalShortcutActiveChanged, qApp,
                     [](QAction *action, bool active) {
                         if (!active && actions.value(action->objectName()) == action) {
                             notify("onShortcut", action->objectName(), false);
                         }
                     });
    QObject::connect(KGlobalAccel::self(), &KGlobalAccel::globalShortcutChanged, qApp,
                     [](QAction *action, const QKeySequence &sequence) {
                         if (actions.value(action->objectName()) == action) {
                             notify("onShortcutKeysChanged", action->objectName(),
                                    sequence.toString(QKeySequence::PortableText));
                         }
                     });
}

bool globalShortcutsAvailable()
{
    const auto *bus = QDBusConnection::sessionBus().interface();
    const QString service = QStringLiteral("org.kde.kglobalaccel");
    return bus && (bus->isServiceRegistered(service) || bus->activatableServiceNames().value().contains(service));
}

bool setGlobalShortcut(const QString &id, const QString &label, const QString &keys)
{
    if (fake()) {
        return true;
    }
    QAction *action = actions.value(id);
    if (!action) {
        action = new QAction(qApp);
        action->setObjectName(id);
        action->setProperty("componentName", component);
        action->setProperty("componentDisplayName", QStringLiteral("Hepcat"));
        QObject::connect(action, &QAction::triggered, action, [id] { notify("onShortcut", id, true); });
        actions.insert(id, action);
    }
    action->setText(label);
    const QList<QKeySequence> sequence = keys.isEmpty() ? QList<QKeySequence>() : QList<QKeySequence>{parse(keys)};
    KGlobalAccel::self()->setDefaultShortcut(action, sequence, KGlobalAccel::NoAutoloading);
    return KGlobalAccel::self()->setShortcut(action, sequence, KGlobalAccel::NoAutoloading);
}

void removeGlobalShortcut(const QString &id)
{
    if (QAction *action = actions.take(id)) {
        KGlobalAccel::self()->removeAllShortcuts(action);
        delete action;
    }
}

QString keySequenceToText(const QVariant &sequence)
{
    return sequence.value<QKeySequence>().toString(QKeySequence::PortableText);
}

QVariant keySequenceFromText(const QString &keys)
{
    return QVariant::fromValue(parse(keys));
}

QString keySequenceNativeText(const QString &keys)
{
    return parse(keys).toString(QKeySequence::NativeText);
}
