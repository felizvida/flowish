#include "DesktopPayloadPolicy.h"

DesktopPayloadDecision evaluateDesktopPayloadTransition(
    const QVariantMap &currentSnapshot,
    const QVariantMap &parsedPayload,
    bool replaceSnapshotOnError) {
    Q_UNUSED(currentSnapshot);
    Q_UNUSED(replaceSnapshotOnError);

    DesktopPayloadDecision decision;
    const QString status = parsedPayload.value("status").toString();
    if (status == "ready") {
        decision.success = true;
        decision.shouldReplaceSnapshot = true;
        decision.shouldRebuildDerivedState = true;
        decision.shouldEmitSnapshotChanged = true;
        decision.status = status;
        return decision;
    }

    decision.success = false;
    decision.shouldReplaceSnapshot = false;
    decision.shouldRebuildDerivedState = false;
    decision.shouldEmitSnapshotChanged = true;
    decision.status = status.isEmpty() ? "error" : status;
    decision.errorMessage = parsedPayload.value("message").toString();
    return decision;
}
