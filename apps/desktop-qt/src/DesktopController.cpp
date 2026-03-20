#include "DesktopController.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonParseError>
#include <QtGlobal>
#include <QStringList>
#include <cmath>

extern "C" {
void *flowjoish_desktop_session_new();
char *flowjoish_desktop_session_snapshot_json(void *session);
char *flowjoish_desktop_session_dispatch_json(void *session, const char *commandJson);
char *flowjoish_desktop_session_reset(void *session);
char *flowjoish_desktop_session_undo(void *session);
char *flowjoish_desktop_session_redo(void *session);
void flowjoish_desktop_session_free(void *session);
void flowjoish_string_free(char *ptr);
}

namespace {

QString takeRustString(char *raw) {
    if (raw == nullptr) {
        return QString();
    }

    const QString text = QString::fromUtf8(raw);
    flowjoish_string_free(raw);
    return text;
}

}  // namespace

DesktopController::DesktopController(QObject *parent) : QObject(parent) {
    session_ = flowjoish_desktop_session_new();
    if (session_ == nullptr) {
        status_ = "error";
        setLastError("Failed to create Rust desktop session");
        emit snapshotChanged();
        return;
    }

    applyRustPayload(takeRustString(flowjoish_desktop_session_snapshot_json(session_)), true);
}

DesktopController::~DesktopController() {
    if (session_ != nullptr) {
        flowjoish_desktop_session_free(session_);
        session_ = nullptr;
    }
}

QString DesktopController::status() const {
    return status_;
}

int DesktopController::commandCount() const {
    return commandCount_;
}

bool DesktopController::canUndo() const {
    return canUndo_;
}

bool DesktopController::canRedo() const {
    return canRedo_;
}

QString DesktopController::commandLogHash() const {
    return commandLogHash_;
}

QString DesktopController::executionHash() const {
    return executionHash_;
}

QVariantList DesktopController::populations() const {
    return populations_;
}

QVariantList DesktopController::commands() const {
    return commands_;
}

QVariantList DesktopController::plots() const {
    return plots_;
}

QString DesktopController::selectedPopulationKey() const {
    return selectedPopulationKey_;
}

QString DesktopController::lastError() const {
    return lastError_;
}

void DesktopController::setSelectedPopulationKey(const QString &populationKey) {
    if (populationKey == selectedPopulationKey_) {
        return;
    }

    selectedPopulationKey_ = populationKey;
    rebuildDerivedState();
    emit selectedPopulationKeyChanged();
    emit snapshotChanged();
}

bool DesktopController::dispatchCommandJson(const QString &commandJson) {
    if (session_ == nullptr) {
        setLastError("Desktop session is unavailable");
        return false;
    }

    const QByteArray utf8 = commandJson.toUtf8();
    return applyRustPayload(
        takeRustString(flowjoish_desktop_session_dispatch_json(session_, utf8.constData())));
}

void DesktopController::applyPresetCommand(const QString &presetId) {
    const QString commandJson = buildPresetCommandJson(presetId);
    if (!commandJson.isEmpty()) {
        dispatchCommandJson(commandJson);
    }
}

void DesktopController::resetSession() {
    if (session_ == nullptr) {
        setLastError("Desktop session is unavailable");
        return;
    }

    applyRustPayload(takeRustString(flowjoish_desktop_session_reset(session_)), true);
}

void DesktopController::undo() {
    if (session_ == nullptr) {
        setLastError("Desktop session is unavailable");
        return;
    }

    applyRustPayload(takeRustString(flowjoish_desktop_session_undo(session_)), true);
}

void DesktopController::redo() {
    if (session_ == nullptr) {
        setLastError("Desktop session is unavailable");
        return;
    }

    applyRustPayload(takeRustString(flowjoish_desktop_session_redo(session_)), true);
}

bool DesktopController::hasPopulation(const QString &populationKey) const {
    for (const QVariant &value : populations_) {
        const QVariantMap population = value.toMap();
        if (population.value("key").toString() == populationKey) {
            return true;
        }
    }
    return false;
}

bool DesktopController::createRectangleGateForPlot(
    const QString &plotId,
    double xMin,
    double xMax,
    double yMin,
    double yMax) {
    const QVariantMap plot = plotDefinition(plotId);
    if (plot.isEmpty()) {
        setLastError(QStringLiteral("Unknown plot '%1'").arg(plotId));
        return false;
    }

    const bool allFinite = std::isfinite(xMin) && std::isfinite(xMax) && std::isfinite(yMin)
        && std::isfinite(yMax);
    if (!allFinite || qFuzzyCompare(xMin, xMax) || qFuzzyCompare(yMin, yMax)) {
        setLastError("Gate drag was too small to create a deterministic rectangle");
        return false;
    }

    const QString populationId = nextInteractivePopulationId(plotId);
    QJsonObject command;
    command.insert("kind", "rectangle_gate");
    const QString sampleId = snapshot_.value("sample").toMap().value("id").toString();
    command.insert(
        "sample_id",
        sampleId.isEmpty() ? QStringLiteral("desktop-demo") : sampleId);
    command.insert("population_id", populationId);
    if (selectedPopulationKey_ == "__all__") {
        command.insert("parent_population", QJsonValue());
    } else {
        command.insert("parent_population", selectedPopulationKey_);
    }
    command.insert("x_channel", plot.value("x_channel").toString());
    command.insert("y_channel", plot.value("y_channel").toString());
    command.insert("x_min", qMin(xMin, xMax));
    command.insert("x_max", qMax(xMin, xMax));
    command.insert("y_min", qMin(yMin, yMax));
    command.insert("y_max", qMax(yMin, yMax));

    return commitInteractiveCommand(command, populationId);
}

bool DesktopController::createPolygonGateForPlot(
    const QString &plotId,
    const QVariantList &vertices) {
    const QVariantMap plot = plotDefinition(plotId);
    if (plot.isEmpty()) {
        setLastError(QStringLiteral("Unknown plot '%1'").arg(plotId));
        return false;
    }

    if (vertices.size() < 3) {
        setLastError("Polygon gates require at least three vertices");
        return false;
    }

    QJsonArray jsonVertices;
    for (const QVariant &value : vertices) {
        const QVariantMap vertex = value.toMap();
        const double x = vertex.value("x").toDouble();
        const double y = vertex.value("y").toDouble();
        if (!std::isfinite(x) || !std::isfinite(y)) {
            setLastError("Polygon gate vertices must stay finite");
            return false;
        }
        jsonVertices.push_back(QJsonObject{{"x", x}, {"y", y}});
    }

    const QString populationId = nextInteractivePopulationId(plotId);
    QJsonObject command;
    command.insert("kind", "polygon_gate");
    const QString sampleId = snapshot_.value("sample").toMap().value("id").toString();
    command.insert(
        "sample_id",
        sampleId.isEmpty() ? QStringLiteral("desktop-demo") : sampleId);
    command.insert("population_id", populationId);
    if (selectedPopulationKey_ == "__all__") {
        command.insert("parent_population", QJsonValue());
    } else {
        command.insert("parent_population", selectedPopulationKey_);
    }
    command.insert("x_channel", plot.value("x_channel").toString());
    command.insert("y_channel", plot.value("y_channel").toString());
    command.insert("vertices", jsonVertices);

    return commitInteractiveCommand(command, populationId);
}

bool DesktopController::commitInteractiveCommand(
    const QJsonObject &command,
    const QString &populationId) {
    const QString payload = QString::fromUtf8(
        QJsonDocument(command).toJson(QJsonDocument::Compact));
    if (!dispatchCommandJson(payload)) {
        return false;
    }

    selectedPopulationKey_ = populationId;
    rebuildDerivedState();
    emit selectedPopulationKeyChanged();
    emit snapshotChanged();
    return true;
}

bool DesktopController::applyRustPayload(const QString &payload, bool replaceSnapshotOnError) {
    if (payload.isEmpty()) {
        setLastError("Rust bridge returned an empty payload");
        return false;
    }

    QJsonParseError parseError;
    const QJsonDocument document = QJsonDocument::fromJson(payload.toUtf8(), &parseError);
    if (parseError.error != QJsonParseError::NoError || !document.isObject()) {
        setLastError(QStringLiteral("Failed to parse Rust payload: %1").arg(parseError.errorString()));
        return false;
    }

    const QVariantMap parsed = document.object().toVariantMap();
    const QString status = parsed.value("status").toString();
    if (status != "ready") {
        status_ = status.isEmpty() ? "error" : status;
        setLastError(parsed.value("message").toString());
        if (replaceSnapshotOnError) {
            snapshot_ = parsed;
            rebuildDerivedState();
            emit snapshotChanged();
        }
        return false;
    }

    snapshot_ = parsed;
    status_ = status;
    setLastError(QString());
    rebuildDerivedState();
    emit snapshotChanged();
    return true;
}

void DesktopController::rebuildDerivedState() {
    commandCount_ = snapshot_.value("command_count").toInt();
    canUndo_ = snapshot_.value("can_undo").toBool();
    canRedo_ = snapshot_.value("can_redo").toBool();
    commandLogHash_ = snapshot_.value("command_log_hash").toString();
    executionHash_ = snapshot_.value("execution_hash").toString();
    commands_ = snapshot_.value("commands").toList();
    populations_ = snapshot_.value("populations").toList();

    QStringList populationKeys;
    for (const QVariant &value : populations_) {
        populationKeys.push_back(value.toMap().value("key").toString());
    }
    if (!populationKeys.contains(selectedPopulationKey_)) {
        selectedPopulationKey_ = populationKeys.contains("__all__") ? "__all__" : populationKeys.value(0);
        emit selectedPopulationKeyChanged();
    }

    plots_.clear();
    for (const QVariant &value : snapshot_.value("plots").toList()) {
        QVariantMap plot = value.toMap();
        const QVariantMap populationPoints = plot.value("population_points").toMap();
        QVariantList highlightPoints;
        if (selectedPopulationKey_ == "__all__") {
            highlightPoints = plot.value("all_points").toList();
        } else {
            highlightPoints = populationPoints.value(selectedPopulationKey_).toList();
        }
        plot.insert("highlight_points", highlightPoints);
        plot.insert("highlight_count", highlightPoints.size());
        plots_.push_back(plot);
    }
}

void DesktopController::setLastError(const QString &message) {
    if (message == lastError_) {
        return;
    }

    lastError_ = message;
    emit lastErrorChanged();
}

QString DesktopController::buildPresetCommandJson(const QString &presetId) const {
    QJsonObject command;
    command.insert("sample_id", QStringLiteral("desktop-demo"));

    if (presetId == "lymphocytes") {
        command.insert("kind", "rectangle_gate");
        command.insert("population_id", "lymphocytes");
        command.insert("parent_population", QJsonValue());
        command.insert("x_channel", "FSC-A");
        command.insert("y_channel", "SSC-A");
        command.insert("x_min", 0.0);
        command.insert("x_max", 35.0);
        command.insert("y_min", 0.0);
        command.insert("y_max", 35.0);
    } else if (presetId == "cd3_cd4") {
        QJsonArray vertices;
        vertices.push_back(QJsonObject{{"x", 0.0}, {"y", 7.0}});
        vertices.push_back(QJsonObject{{"x", 6.0}, {"y", 7.0}});
        vertices.push_back(QJsonObject{{"x", 6.0}, {"y", 10.0}});
        vertices.push_back(QJsonObject{{"x", 0.0}, {"y", 10.0}});

        command.insert("kind", "polygon_gate");
        command.insert("population_id", "cd3_cd4");
        command.insert("parent_population", "lymphocytes");
        command.insert("x_channel", "CD3");
        command.insert("y_channel", "CD4");
        command.insert("vertices", vertices);
    } else {
        return QString();
    }

    return QString::fromUtf8(QJsonDocument(command).toJson(QJsonDocument::Compact));
}

QVariantMap DesktopController::plotDefinition(const QString &plotId) const {
    for (const QVariant &value : snapshot_.value("plots").toList()) {
        const QVariantMap plot = value.toMap();
        if (plot.value("id").toString() == plotId) {
            return plot;
        }
    }
    return {};
}

QString DesktopController::nextInteractivePopulationId(const QString &plotId) const {
    QStringList segments;
    if (selectedPopulationKey_ != "__all__") {
        segments.push_back(sanitizePopulationSegment(selectedPopulationKey_));
    }
    segments.push_back(sanitizePopulationSegment(plotId));
    segments.push_back(QStringLiteral("gate"));
    segments.push_back(QString::number(commandCount_ + 1));
    return segments.join(QStringLiteral("_"));
}

QString DesktopController::sanitizePopulationSegment(const QString &value) {
    QString result;
    result.reserve(value.size());
    bool previousWasUnderscore = false;
    for (const QChar &character : value) {
        const QChar lowered = character.toLower();
        if (lowered.isLetterOrNumber()) {
            result.push_back(lowered);
            previousWasUnderscore = false;
            continue;
        }

        if (!previousWasUnderscore && !result.isEmpty()) {
            result.push_back('_');
            previousWasUnderscore = true;
        }
    }

    while (result.endsWith('_')) {
        result.chop(1);
    }
    return result.isEmpty() ? QStringLiteral("gate") : result;
}
