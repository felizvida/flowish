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

struct DesktopComparisonRefreshDecision {
    bool shouldClearComparison = false;
    bool shouldRequestRefresh = false;
    QString cacheKey;
};

DesktopPayloadDecision evaluateDesktopPayloadTransition(
    const QVariantMap &currentSnapshot,
    const QVariantMap &parsedPayload,
    bool replaceSnapshotOnError);

QString buildDesktopComparisonCacheKey(
    const QVariantMap &snapshot,
    const QString &populationKey,
    const QString &status);

DesktopComparisonRefreshDecision evaluateDesktopComparisonRefresh(
    const QVariantMap &snapshot,
    const QString &populationKey,
    const QString &status,
    const QString &currentCacheKey,
    const QString &pendingCacheKey);
