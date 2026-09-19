#include "pointer_guard.hpp"

#include <QtCore/QDateTime>
#include <QtCore/QDir>
#include <QtCore/QFile>
#include <QtCore/QPointer>
#include <QtCore/QSet>
#include <QtCore/QTimer>
#include <QtGui/QGuiApplication>
#include <QtGui/QPointerEvent>
#include <QtGui/QWindow>
#include <QtGui/private/qpointingdevice_p.h>
#include <QtQuick/QQuickItem>
#include <QtQuick/QQuickWindow>

#include <cstdio>
#include <cstring>

namespace {

bool pointerDebug()
{
    static int cached = -1;
    if (cached < 0) {
        const QByteArray value = qgetenv("QINGYIN_POINTER_DEBUG").toLower();
        cached = (value == "1" || value == "true" || value == "yes" || value == "on") ? 1 : 0;
    }
    return cached == 1;
}

void pointerLog(const char *kind, const QString &detail)
{
    if (!pointerDebug())
        return;
    const qint64 millis = QDateTime::currentMSecsSinceEpoch();
    const QByteArray line = QByteArray("[qingyin-pointer] ")
        + QByteArray::number(millis) + "  " + kind + "  " + detail.toUtf8();
    std::fprintf(stderr, "%s\n", line.constData());
    QFile file(QDir::temp().filePath(QStringLiteral("qingyin-pointer.log")));
    if (file.open(QIODevice::WriteOnly | QIODevice::Append)) {
        file.write(line);
        file.write("\n");
    }
}

QString describe(const QObject *object)
{
    if (!object)
        return QStringLiteral("none");
    const char *className = object->metaObject()->className();
    QString name = object->objectName();
    if (name.isEmpty()) {
        if (const auto *item = qobject_cast<const QQuickItem *>(object)) {
            if (QQuickItem *parent = item->parentItem())
                name = parent->objectName();
        } else if (QObject *parent = object->parent()) {
            name = parent->objectName();
        }
    }
    return name.isEmpty()
        ? QString::fromLatin1(className)
        : (QString::fromLatin1(className) + QLatin1Char('/') + name);
}

void disarmItem(QQuickItem *item)
{
    if (item->inherits("QQuickFlickable")) {
        item->setAcceptedMouseButtons(Qt::NoButton);
        item->setKeepMouseGrab(false);
        item->ungrabTouchPoints();
        item->ungrabMouse();
    } else if (item->keepMouseGrab()) {
        item->setKeepMouseGrab(false);
        item->ungrabMouse();
    }
    const auto children = item->childItems();
    for (QQuickItem *child : children)
        disarmItem(child);
}

QString dropPointerGrabs()
{
    if (!QGuiApplication::instance())
        return QStringLiteral("idle no-app");

    QStringList parts;
    bool dropped = false;

    const auto windows = QGuiApplication::topLevelWindows();
    for (QWindow *window : windows) {
        auto *quick = qobject_cast<QQuickWindow *>(window);
        if (!quick)
            continue;
        QQuickItem *grabber = quick->mouseGrabberItem();
        parts.append(QStringLiteral("mouse=") + describe(grabber));
        if (QQuickItem *root = quick->contentItem())
            disarmItem(root);
        if (grabber) {
            dropped = true;
            grabber->setKeepMouseGrab(false);
            grabber->ungrabMouse();
            grabber->ungrabTouchPoints();
        }
    }

    const auto devices = QInputDevice::devices();
    for (const QInputDevice *device : devices) {
        auto *pointing = qobject_cast<const QPointingDevice *>(device);
        if (!pointing)
            continue;
        auto *priv = QPointingDevicePrivate::get(const_cast<QPointingDevice *>(pointing));
        if (!priv)
            continue;
        int guard = 0;
        while (QObject *exclusive = priv->firstPointExclusiveGrabber()) {
            parts.append(QStringLiteral("exclusive=") + describe(exclusive));
            dropped = true;
            priv->removeGrabber(exclusive, true);
            if (++guard > 8)
                break;
        }
    }

    if (dropped) {
        for (QWindow *window : windows) {
            auto *quick = qobject_cast<QQuickWindow *>(window);
            if (!quick)
                continue;
            if (QQuickItem *root = quick->contentItem()) {
                root->ungrabMouse();
                root->ungrabTouchPoints();
            }
            quick->unsetCursor();
        }
        parts.prepend(QStringLiteral("dropped"));
    } else {
        parts.prepend(QStringLiteral("idle"));
    }
    return parts.join(QLatin1Char(' '));
}

bool hasHeldGrab()
{
    if (!QGuiApplication::instance())
        return false;
    for (QWindow *window : QGuiApplication::topLevelWindows()) {
        if (auto *quick = qobject_cast<QQuickWindow *>(window)) {
            if (quick->mouseGrabberItem())
                return true;
        }
    }
    for (const QInputDevice *device : QInputDevice::devices()) {
        auto *pointing = qobject_cast<const QPointingDevice *>(device);
        if (!pointing)
            continue;
        auto *priv = QPointingDevicePrivate::get(const_cast<QPointingDevice *>(pointing));
        if (priv && priv->firstPointExclusiveGrabber())
            return true;
    }
    return false;
}

void cancelGrabber(QObject *grabber)
{
    if (!grabber)
        return;
    for (const QInputDevice *device : QInputDevice::devices()) {
        auto *pointing = qobject_cast<const QPointingDevice *>(device);
        if (!pointing)
            continue;
        auto *priv = QPointingDevicePrivate::get(const_cast<QPointingDevice *>(pointing));
        if (priv)
            priv->removeGrabber(grabber, true);
    }
    if (auto *item = qobject_cast<QQuickItem *>(grabber)) {
        item->setKeepMouseGrab(false);
        if (item->inherits("QQuickFlickable"))
            item->setAcceptedMouseButtons(Qt::NoButton);
        item->ungrabMouse();
        item->ungrabTouchPoints();
    }
}

void hookGrabLogging(const QPointingDevice *device)
{
    static QSet<qint64> hooked;
    if (!device || hooked.contains(device->systemId()))
        return;
    hooked.insert(device->systemId());
    QObject::connect(
        device,
        &QPointingDevice::grabChanged,
        qGuiApp,
        [](QObject *grabber, QPointingDevice::GrabTransition transition,
           const QPointerEvent *, const QEventPoint &) {
            if (transition != QPointingDevice::GrabExclusive
                && transition != QPointingDevice::UngrabExclusive
                && transition != QPointingDevice::CancelGrabExclusive) {
                return;
            }
            pointerLog(
                "qt-grab",
                QString::number(int(transition)) + QLatin1Char(' ') + describe(grabber));
            if (transition == QPointingDevice::GrabExclusive
                && grabber
                && grabber->inherits("QQuickFlickable")) {
                QPointer<QObject> stuck(grabber);
                QTimer::singleShot(0, qGuiApp, [stuck] {
                    if (!stuck)
                        return;
                    pointerLog("flickable-cancel", describe(stuck.data()));
                    cancelGrabber(stuck.data());
                });
            }
        });
}

void hookDevices()
{
    if (const QPointingDevice *primary = QPointingDevice::primaryPointingDevice())
        hookGrabLogging(primary);
    for (const QInputDevice *device : QInputDevice::devices()) {
        if (auto *pointing = qobject_cast<const QPointingDevice *>(device))
            hookGrabLogging(pointing);
    }
}

class PointerFilter : public QObject
{
public:
    explicit PointerFilter(QObject *parent = nullptr)
        : QObject(parent)
    {
    }

    bool eventFilter(QObject *, QEvent *event) override
    {
        switch (event->type()) {
        case QEvent::MouseButtonPress:
        case QEvent::TouchBegin:
            hookDevices();
            if (hasHeldGrab()) {
                const QString report = dropPointerGrabs();
                pointerLog("press-cancel", report);
            }
            break;
        case QEvent::Leave:
        case QEvent::UngrabMouse:
        case QEvent::WindowDeactivate:
        case QEvent::TouchCancel:
            QTimer::singleShot(0, qGuiApp, [] {
                if (!hasHeldGrab())
                    return;
                const QString report = dropPointerGrabs();
                pointerLog("leave-cancel", report);
            });
            break;
        case QEvent::MouseButtonRelease:
        case QEvent::TouchEnd:
            QTimer::singleShot(0, qGuiApp, [] {
                if (!hasHeldGrab())
                    return;
                const QString report = dropPointerGrabs();
                pointerLog("release-cancel", report);
            });
            break;
        default:
            break;
        }
        return false;
    }
};

int copyReport(const QString &report, char *out, int outLen)
{
    const QByteArray utf8 = report.toUtf8();
    if (!out || outLen <= 0)
        return 0;
    const int n = qMin(outLen - 1, int(utf8.size()));
    std::memcpy(out, utf8.constData(), size_t(n));
    out[n] = '\0';
    return n;
}

} // namespace

extern "C" {

void qingyin_install_pointer_guard()
{
    if (!QGuiApplication::instance())
        return;
    static bool installed = false;
    if (installed)
        return;
    installed = true;
    qGuiApp->installEventFilter(new PointerFilter(qGuiApp));
    hookDevices();
    const QString report = dropPointerGrabs();
    pointerLog("guard-install", report);
}

void qingyin_drop_pointer_grabs_later()
{
    if (!QGuiApplication::instance())
        return;
    QTimer::singleShot(0, qGuiApp, [] {
        const QString report = dropPointerGrabs();
        pointerLog("grabber-later", report);
    });
}

int qingyin_drop_pointer_grabs(char *out, int out_len)
{
    return copyReport(dropPointerGrabs(), out, out_len);
}

}
