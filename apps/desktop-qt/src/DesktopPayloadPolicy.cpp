#include "DesktopPayloadPolicy.h"

#include <QStringList>

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

QString buildDesktopComparisonCacheKey(
    const QVariantMap &snapshot,
    const QString &populationKey,
    const QString &status) {
    if (status != "ready") {
        return QString();
    }

    QString comparisonStateHash = snapshot.value("comparison_state_hash").toString();
    if (comparisonStateHash.isEmpty()) {
        comparisonStateHash = snapshot.value("execution_hash").toString();
    }
    if (comparisonStateHash.isEmpty()) {
        return QString();
    }

    const QString sampleId = snapshot.value("sample").toMap().value("id").toString();
    const QString normalizedPopulation =
        populationKey.trimmed().isEmpty() ? QStringLiteral("__all__") : populationKey;

    return QStringList{sampleId, normalizedPopulation, comparisonStateHash}.join("|");
}

DesktopComparisonRefreshDecision evaluateDesktopComparisonRefresh(
    const QVariantMap &snapshot,
    const QString &populationKey,
    const QString &status,
    const QString &currentCacheKey,
    const QString &pendingCacheKey) {
    DesktopComparisonRefreshDecision decision;
    decision.cacheKey = buildDesktopComparisonCacheKey(snapshot, populationKey, status);

    if (decision.cacheKey.isEmpty()) {
        decision.shouldClearComparison =
            !currentCacheKey.isEmpty() || !pendingCacheKey.isEmpty();
        return decision;
    }

    if (decision.cacheKey == currentCacheKey || decision.cacheKey == pendingCacheKey) {
        return decision;
    }

    decision.shouldClearComparison =
        !currentCacheKey.isEmpty()
        || (!pendingCacheKey.isEmpty() && pendingCacheKey != decision.cacheKey);
    decision.shouldRequestRefresh = true;
    return decision;
}
