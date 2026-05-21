#include "../src/DesktopFigureExport.h"
#include "../src/DesktopPayloadPolicy.h"

#include <QList>
#include <QRectF>
#include <QSizeF>
#include <QtGlobal>
#include <QVariantMap>

namespace {

int expect(bool condition, const char *message) {
    if (!condition) {
        return message[0] == '\0' ? 1 : 1;
    }
    return 0;
}

bool closeTo(qreal actual, qreal expected) {
    return qAbs(actual - expected) < 0.001;
}

}  // namespace

int main() {
    {
        QVariantMap currentSnapshot{
            {"status", "ready"},
            {"sample", QVariantMap{{"id", "sample-a"}}},
        };
        QVariantMap errorPayload{
            {"status", "error"},
            {"message", "workspace load failed"},
        };

        const DesktopPayloadDecision decision = evaluateDesktopPayloadTransition(
            currentSnapshot,
            errorPayload,
            true);
        if (expect(!decision.success, "error payload should fail")) {
            return 1;
        }
        if (expect(!decision.shouldReplaceSnapshot, "error payload should not replace snapshot")) {
            return 1;
        }
        if (expect(!decision.shouldRebuildDerivedState, "error payload should not rebuild state")) {
            return 1;
        }
        if (expect(decision.shouldEmitSnapshotChanged, "error payload should still notify")) {
            return 1;
        }
        if (expect(decision.status == "error", "error status should be preserved")) {
            return 1;
        }
        if (expect(decision.errorMessage == "workspace load failed", "error message should propagate")) {
            return 1;
        }
    }

    {
        QVariantMap currentSnapshot;
        QVariantMap readyPayload{
            {"status", "ready"},
            {"sample", QVariantMap{{"id", "sample-b"}}},
        };

        const DesktopPayloadDecision decision = evaluateDesktopPayloadTransition(
            currentSnapshot,
            readyPayload,
            true);
        if (expect(decision.success, "ready payload should succeed")) {
            return 1;
        }
        if (expect(decision.shouldReplaceSnapshot, "ready payload should replace snapshot")) {
            return 1;
        }
        if (expect(decision.shouldRebuildDerivedState, "ready payload should rebuild state")) {
            return 1;
        }
        if (expect(decision.shouldEmitSnapshotChanged, "ready payload should notify")) {
            return 1;
        }
        if (expect(decision.status == "ready", "ready status should propagate")) {
            return 1;
        }
        if (expect(decision.errorMessage.isEmpty(), "ready payload should clear error")) {
            return 1;
        }
    }

    {
        QVariantMap snapshot{
            {"status", "ready"},
            {"sample", QVariantMap{{"id", "sample-a"}}},
            {"comparison_state_hash", "comparison-1"},
            {"execution_hash", "view-1"},
        };
        QVariantMap viewOnlySnapshot = snapshot;
        viewOnlySnapshot.insert("execution_hash", "view-2");

        const QString first = buildDesktopComparisonCacheKey(
            snapshot,
            "lymphocytes",
            "ready");
        const QString viewOnly = buildDesktopComparisonCacheKey(
            viewOnlySnapshot,
            "lymphocytes",
            "ready");
        if (expect(first == viewOnly, "view-only hashes should not change comparison key")) {
            return 1;
        }

        const QString differentPopulation = buildDesktopComparisonCacheKey(
            snapshot,
            "cd3_cd4",
            "ready");
        if (expect(first != differentPopulation, "population changes should change comparison key")) {
            return 1;
        }

        QVariantMap differentActiveSample = snapshot;
        differentActiveSample.insert("sample", QVariantMap{{"id", "sample-b"}});
        const QString differentSample = buildDesktopComparisonCacheKey(
            differentActiveSample,
            "lymphocytes",
            "ready");
        if (expect(first != differentSample, "active sample changes should change comparison key")) {
            return 1;
        }
    }

    {
        QVariantMap errorSnapshot{
            {"sample", QVariantMap{{"id", "sample-a"}}},
            {"comparison_state_hash", "comparison-1"},
        };
        if (expect(
                buildDesktopComparisonCacheKey(errorSnapshot, "lymphocytes", "error").isEmpty(),
                "non-ready snapshots should not build comparison cache keys")) {
            return 1;
        }
    }

    {
        QVariantMap snapshot{
            {"status", "ready"},
            {"sample", QVariantMap{{"id", "sample-a"}}},
            {"comparison_state_hash", "comparison-1"},
        };
        const QString existingKey = buildDesktopComparisonCacheKey(
            snapshot,
            "lymphocytes",
            "ready");

        DesktopComparisonRefreshDecision unchanged =
            evaluateDesktopComparisonRefresh(
                snapshot,
                "lymphocytes",
                "ready",
                existingKey,
                QString());
        if (expect(!unchanged.shouldRequestRefresh, "matching cache key should not refresh")) {
            return 1;
        }
        if (expect(!unchanged.shouldClearComparison, "matching cache key should not clear")) {
            return 1;
        }

        DesktopComparisonRefreshDecision pending =
            evaluateDesktopComparisonRefresh(
                snapshot,
                "lymphocytes",
                "ready",
                QString(),
                existingKey);
        if (expect(!pending.shouldRequestRefresh, "pending cache key should not refresh twice")) {
            return 1;
        }

        const QString stalePendingKey = buildDesktopComparisonCacheKey(
            snapshot,
            "cd3_cd4",
            "ready");
        DesktopComparisonRefreshDecision replacesPending =
            evaluateDesktopComparisonRefresh(
                snapshot,
                "lymphocytes",
                "ready",
                QString(),
                stalePendingKey);
        if (expect(replacesPending.shouldRequestRefresh, "stale pending key should be replaced")) {
            return 1;
        }
        if (expect(replacesPending.shouldClearComparison, "stale pending key should be cleared")) {
            return 1;
        }

        DesktopComparisonRefreshDecision changed =
            evaluateDesktopComparisonRefresh(
                snapshot,
                "cd3_cd4",
                "ready",
                existingKey,
                QString());
        if (expect(changed.shouldRequestRefresh, "changed population should request refresh")) {
            return 1;
        }
        if (expect(changed.shouldClearComparison, "changed population should clear stale data")) {
            return 1;
        }

        DesktopComparisonRefreshDecision unavailable =
            evaluateDesktopComparisonRefresh(
                snapshot,
                "lymphocytes",
                "error",
                existingKey,
                QString());
        if (expect(!unavailable.shouldRequestRefresh, "error status should not refresh")) {
            return 1;
        }
        if (expect(unavailable.shouldClearComparison, "error status should clear cached comparison")) {
            return 1;
        }
    }

    {
        const QRectF target = fitFigureIntoSlot(QSizeF(1600.0, 900.0), QRectF(0.0, 0.0, 1000.0, 500.0));
        if (expect(closeTo(target.width(), 888.888888889), "wide image should fit by slot height")) {
            return 1;
        }
        if (expect(closeTo(target.height(), 500.0), "wide image should use full slot height")) {
            return 1;
        }
        if (expect(closeTo(target.left(), 55.555555556), "wide image should be centered horizontally")) {
            return 1;
        }
        if (expect(closeTo(target.top(), 0.0), "wide image should align with slot top")) {
            return 1;
        }
    }

    {
        const QRectF contentRect(10.0, 20.0, 1000.0, 600.0);
        const QList<QRectF> rects = stackFigureReportRects(
            QList<QSizeF>{
                QSizeF(1600.0, 900.0),
                QSizeF(1600.0, 900.0),
                QSizeF(800.0, 600.0),
            },
            contentRect,
            20.0);
        if (expect(rects.size() == 3, "report layout should return one rect per image")) {
            return 1;
        }
        if (expect(rects.at(0).top() >= contentRect.top(), "first report rect should stay in bounds")) {
            return 1;
        }
        if (expect(rects.at(2).bottom() <= contentRect.bottom(), "last report rect should stay in bounds")) {
            return 1;
        }
        if (expect(rects.at(0).bottom() < rects.at(1).top(), "first and second report rects should not overlap")) {
            return 1;
        }
        if (expect(rects.at(1).bottom() < rects.at(2).top(), "second and third report rects should not overlap")) {
            return 1;
        }
        if (expect(closeTo(rects.at(0).width() / rects.at(0).height(), 1600.0 / 900.0), "report layout should preserve aspect ratio")) {
            return 1;
        }
    }

    return 0;
}
