#pragma once

#include <QString>
#include <QVariant>

class QObject;

// Global shortcuts through KDE's kglobalaccel service. Activations are passed
// to `onShortcut(id, pressed)` on the QML root object, and changes made in
// System Settings to `onShortcutKeysChanged(id, keys)`.
void setShortcutReceiver(QObject *root);

bool globalShortcutsAvailable();
// `keys` is a key sequence in portable text, e.g. "Ctrl+Alt+M".
bool setGlobalShortcut(const QString &id, const QString &label, const QString &keys);
void removeGlobalShortcut(const QString &id);
// Between the QKeySequence values of KeySequenceItem and portable text.
QString keySequenceToText(const QVariant &sequence);
QVariant keySequenceFromText(const QString &keys);
// For showing: "Ctrl+Alt+M" the way the desktop writes it.
QString keySequenceNativeText(const QString &keys);
