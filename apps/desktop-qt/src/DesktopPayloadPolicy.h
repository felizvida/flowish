#pragma once

#include <QString>
#include <QVariantMap>

struct DesktopPayloadDecision {
    bool success = false;
    bool shouldReplaceSnapshot = false;
    bool shouldRebuildDerivedState = false;
    bool shouldEmitSnapshotChanged = false;
    QString status;
    QString errorMessage;
};

DesktopPayloadDecision evaluateDesktopPayloadTransition(
    const QVariantMap &currentSnapshot,
    const QVariantMap &parsedPayload,
    bool replaceSnapshotOnError);
