#include "../src/DesktopPayloadPolicy.h"

#include <QVariantMap>

namespace {

int expect(bool condition, const char *message) {
    if (!condition) {
        return message[0] == '\0' ? 1 : 1;
    }
    return 0;
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

    return 0;
}
