#include "DesktopController.h"
#include "DesktopPayloadPolicy.h"

#include <QFileDialog>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonParseError>
#include <QSet>
#include <QTimer>
#include <QtGlobal>
#include <QStringList>
#include <algorithm>
#include <cmath>

extern "C" {
void *flowjoish_desktop_session_new();
char *flowjoish_desktop_session_snapshot_json(void *session);
char *flowjoish_desktop_session_dispatch_json(void *session, const char *commandJson);
char *flowjoish_desktop_session_reset(void *session);
char *flowjoish_desktop_session_undo(void *session);
char *flowjoish_desktop_session_redo(void *session);
char *flowjoish_desktop_session_import_fcs_json(void *session, const char *filePathsJson);
char *flowjoish_desktop_session_select_sample(void *session, const char *sampleId);
char *flowjoish_desktop_session_save_workspace(void *session, const char *workspacePath);
char *flowjoish_desktop_session_load_workspace(void *session, const char *workspacePath);
char *flowjoish_desktop_session_export_stats_csv(void *session, const char *exportPath);
char *flowjoish_desktop_session_apply_active_template_to_other_samples(void *session);
char *flowjoish_desktop_session_export_batch_stats_csv(void *session, const char *exportPath);
char *flowjoish_desktop_session_population_comparison_json(void *session, const char *populationKey);
char *flowjoish_desktop_session_export_population_comparison_csv(
    void *session,
    const char *populationKey,
    const char *exportPath);
char *flowjoish_desktop_session_export_population_group_summary_csv(
    void *session,
    const char *populationKey,
    const char *exportPath);
char *flowjoish_desktop_session_set_derived_metric_json(
    void *session,
    const char *metricJson);
char *flowjoish_desktop_session_export_population_derived_metric_csv(
    void *session,
    const char *populationKey,
    const char *exportPath);
char *flowjoish_desktop_session_set_sample_group_label(
    void *session,
    const char *sampleId,
    const char *groupLabel);
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

int pointColumnCount(const QVariantMap &columns) {
    return columns.value(QStringLiteral("event_indices")).toList().size();
}

QVariantMap pointColumnsFromPointMaps(const QVariantList &points) {
    QVariantList eventIndices;
    QVariantList xValues;
    QVariantList yValues;
    eventIndices.reserve(points.size());
    xValues.reserve(points.size());
    yValues.reserve(points.size());

    for (const QVariant &pointValue : points) {
        const QVariantMap point = pointValue.toMap();
        eventIndices.push_back(point.value(QStringLiteral("event_index")));
        xValues.push_back(point.value(QStringLiteral("x")));
        yValues.push_back(point.value(QStringLiteral("y")));
    }

    return QVariantMap{
        {QStringLiteral("event_indices"), eventIndices},
        {QStringLiteral("x_values"), xValues},
        {QStringLiteral("y_values"), yValues},
    };
}

QVariantMap filterPointColumns(
    const QVariantMap &columns,
    const QSet<int> &selectedEventIndices) {
    const QVariantList eventIndices = columns.value(QStringLiteral("event_indices")).toList();
    const QVariantList xValues = columns.value(QStringLiteral("x_values")).toList();
    const QVariantList yValues = columns.value(QStringLiteral("y_values")).toList();
    const int count = std::min({eventIndices.size(), xValues.size(), yValues.size()});

    QVariantList filteredEventIndices;
    QVariantList filteredXValues;
    QVariantList filteredYValues;
    filteredEventIndices.reserve(count);
    filteredXValues.reserve(count);
    filteredYValues.reserve(count);

    for (int index = 0; index < count; ++index) {
        if (!selectedEventIndices.contains(eventIndices.at(index).toInt())) {
            continue;
        }
        filteredEventIndices.push_back(eventIndices.at(index));
        filteredXValues.push_back(xValues.at(index));
        filteredYValues.push_back(yValues.at(index));
    }

    return QVariantMap{
        {QStringLiteral("event_indices"), filteredEventIndices},
        {QStringLiteral("x_values"), filteredXValues},
        {QStringLiteral("y_values"), filteredYValues},
    };
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

QString DesktopController::workspacePath() const {
    return workspacePath_;
}

QVariantMap DesktopController::sample() const {
    return sample_;
}

QVariantList DesktopController::samples() const {
    return samples_;
}

QVariantList DesktopController::analysisActions() const {
    return analysisActions_;
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

QVariantMap DesktopController::derivedMetric() const {
    return derivedMetric_;
}

QVariantMap DesktopController::selectedPopulationStats() const {
    return selectedPopulationStats_;
}

QVariantMap DesktopController::selectedPopulationComparison() const {
    return selectedPopulationComparison_;
}

QString DesktopController::selectedSampleId() const {
    return selectedSampleId_;
}

QString DesktopController::selectedPopulationKey() const {
    return selectedPopulationKey_;
}

QString DesktopController::lastError() const {
    return lastError_;
}

void DesktopController::setSelectedSampleId(const QString &sampleId) {
    if (sampleId.isEmpty() || sampleId == selectedSampleId_) {
        return;
    }

    if (session_ == nullptr) {
        setLastError("Desktop session is unavailable");
        return;
    }

    const QByteArray utf8 = sampleId.toUtf8();
    applyRustPayload(
        takeRustString(flowjoish_desktop_session_select_sample(session_, utf8.constData())),
        true);
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
    if (commandJson.isEmpty()) {
        setLastError(
            QStringLiteral("Preset '%1' is not compatible with the active sample").arg(presetId));
        return;
    }

    dispatchCommandJson(commandJson);
}

bool DesktopController::canApplyPreset(const QString &presetId) const {
    return presetIsAvailable(presetId);
}

void DesktopController::importFcsFiles() {
    const QStringList paths = QFileDialog::getOpenFileNames(
        nullptr,
        tr("Import FCS Files"),
        QString(),
        tr("Flow Cytometry Standard Files (*.fcs);;All Files (*)"));
    if (!paths.isEmpty()) {
        loadSampleFiles(paths);
    }
}

bool DesktopController::loadSampleFiles(const QStringList &paths) {
    if (session_ == nullptr) {
        setLastError("Desktop session is unavailable");
        return false;
    }

    QJsonArray filePaths;
    for (const QString &path : paths) {
        const QString trimmed = path.trimmed();
        if (!trimmed.isEmpty()) {
            filePaths.push_back(trimmed);
        }
    }

    if (filePaths.isEmpty()) {
        setLastError("No FCS files were selected for import");
        return false;
    }

    const QByteArray payload =
        QJsonDocument(filePaths).toJson(QJsonDocument::Compact);
    const bool imported = applyRustPayload(
        takeRustString(
            flowjoish_desktop_session_import_fcs_json(session_, payload.constData())),
        true);
    if (imported) {
        setWorkspacePath(QString());
    }
    return imported;
}

void DesktopController::saveWorkspaceAs() {
    const QString path = QFileDialog::getSaveFileName(
        nullptr,
        tr("Save Workspace"),
        workspacePath_,
        tr("Parallax Workspace (*.parallax.json);;JSON Files (*.json)"));
    if (!path.isEmpty()) {
        saveWorkspaceToFile(path);
    }
}

void DesktopController::exportStatsCsv() {
    const QString suggestedName =
        sample_.value("display_name").toString().isEmpty()
            ? QStringLiteral("parallax-stats.csv")
            : sample_.value("display_name").toString() + QStringLiteral("-stats.csv");
    const QString path = QFileDialog::getSaveFileName(
        nullptr,
        tr("Export Population Stats"),
        suggestedName,
        tr("CSV Files (*.csv);;All Files (*)"));
    if (!path.isEmpty()) {
        exportStatsCsvToFile(path);
    }
}

void DesktopController::applyActiveTemplateToOtherSamples() {
    if (session_ == nullptr) {
        setLastError("Desktop session is unavailable");
        return;
    }

    applyRustPayload(
        takeRustString(
            flowjoish_desktop_session_apply_active_template_to_other_samples(session_)),
        true);
}

void DesktopController::exportBatchStatsCsv() {
    const QString path = QFileDialog::getSaveFileName(
        nullptr,
        tr("Export Batch Population Stats"),
        QStringLiteral("parallax-batch-stats.csv"),
        tr("CSV Files (*.csv);;All Files (*)"));
    if (!path.isEmpty()) {
        exportBatchStatsCsvToFile(path);
    }
}

void DesktopController::exportSelectedPopulationComparisonCsv() {
    const QString populationId =
        selectedPopulationComparison_.value("population_id").toString();
    const QString suggestedName = populationId.isEmpty()
        ? QStringLiteral("parallax-population-comparison.csv")
        : populationId + QStringLiteral("-comparison.csv");
    const QString path = QFileDialog::getSaveFileName(
        nullptr,
        tr("Export Population Comparison"),
        suggestedName,
        tr("CSV Files (*.csv);;All Files (*)"));
    if (!path.isEmpty()) {
        exportSelectedPopulationComparisonCsvToFile(path);
    }
}

void DesktopController::exportSelectedPopulationGroupSummaryCsv() {
    const QString populationId =
        selectedPopulationComparison_.value("population_id").toString();
    const QString suggestedName = populationId.isEmpty()
        ? QStringLiteral("parallax-cohort-summary.csv")
        : populationId + QStringLiteral("-cohort-summary.csv");
    const QString path = QFileDialog::getSaveFileName(
        nullptr,
        tr("Export Cohort Summary"),
        suggestedName,
        tr("CSV Files (*.csv);;All Files (*)"));
    if (!path.isEmpty()) {
        exportSelectedPopulationGroupSummaryCsvToFile(path);
    }
}

void DesktopController::exportSelectedPopulationDerivedMetricCsv() {
    const QString populationId =
        selectedPopulationComparison_.value("population_id").toString();
    const QString suggestedName = populationId.isEmpty()
        ? QStringLiteral("parallax-derived-metric.csv")
        : populationId + QStringLiteral("-derived-metric.csv");
    const QString path = QFileDialog::getSaveFileName(
        nullptr,
        tr("Export Derived Metric"),
        suggestedName,
        tr("CSV Files (*.csv);;All Files (*)"));
    if (!path.isEmpty()) {
        exportSelectedPopulationDerivedMetricCsvToFile(path);
    }
}

bool DesktopController::saveWorkspaceToFile(const QString &path) {
    if (session_ == nullptr) {
        setLastError("Desktop session is unavailable");
        return false;
    }

    const QString trimmed = path.trimmed();
    if (trimmed.isEmpty()) {
        setLastError("Workspace path cannot be empty");
        return false;
    }

    const QByteArray utf8 = trimmed.toUtf8();
    const bool saved = applyRustPayload(
        takeRustString(
            flowjoish_desktop_session_save_workspace(session_, utf8.constData())),
        true);
    if (saved) {
        setWorkspacePath(trimmed);
    }
    return saved;
}

bool DesktopController::exportStatsCsvToFile(const QString &path) {
    if (session_ == nullptr) {
        setLastError("Desktop session is unavailable");
        return false;
    }

    const QString trimmed = path.trimmed();
    if (trimmed.isEmpty()) {
        setLastError("Stats export path cannot be empty");
        return false;
    }

    const QByteArray utf8 = trimmed.toUtf8();
    return applyRustPayload(
        takeRustString(
            flowjoish_desktop_session_export_stats_csv(session_, utf8.constData())),
        true);
}

bool DesktopController::exportBatchStatsCsvToFile(const QString &path) {
    if (session_ == nullptr) {
        setLastError("Desktop session is unavailable");
        return false;
    }

    const QString trimmed = path.trimmed();
    if (trimmed.isEmpty()) {
        setLastError("Batch stats export path cannot be empty");
        return false;
    }

    const QByteArray utf8 = trimmed.toUtf8();
    return applyRustPayload(
        takeRustString(
            flowjoish_desktop_session_export_batch_stats_csv(session_, utf8.constData())),
        true);
}

bool DesktopController::exportSelectedPopulationComparisonCsvToFile(const QString &path) {
    if (session_ == nullptr) {
        setLastError("Desktop session is unavailable");
        return false;
    }

    const QString trimmed = path.trimmed();
    if (trimmed.isEmpty()) {
        setLastError("Population comparison export path cannot be empty");
        return false;
    }

    const QByteArray populationKeyUtf8 =
        (selectedPopulationKey_.isEmpty() ? QStringLiteral("__all__") : selectedPopulationKey_)
            .toUtf8();
    const QByteArray pathUtf8 = trimmed.toUtf8();
    return applyRustPayload(
        takeRustString(flowjoish_desktop_session_export_population_comparison_csv(
            session_,
            populationKeyUtf8.constData(),
            pathUtf8.constData())),
        true);
}

bool DesktopController::exportSelectedPopulationGroupSummaryCsvToFile(const QString &path) {
    if (session_ == nullptr) {
        setLastError("Desktop session is unavailable");
        return false;
    }

    const QString trimmed = path.trimmed();
    if (trimmed.isEmpty()) {
        setLastError("Population group summary export path cannot be empty");
        return false;
    }

    const QByteArray populationKeyUtf8 =
        (selectedPopulationKey_.isEmpty() ? QStringLiteral("__all__") : selectedPopulationKey_)
            .toUtf8();
    const QByteArray pathUtf8 = trimmed.toUtf8();
    return applyRustPayload(
        takeRustString(flowjoish_desktop_session_export_population_group_summary_csv(
            session_,
            populationKeyUtf8.constData(),
            pathUtf8.constData())),
        true);
}

bool DesktopController::exportSelectedPopulationDerivedMetricCsvToFile(const QString &path) {
    if (session_ == nullptr) {
        setLastError("Desktop session is unavailable");
        return false;
    }

    const QString trimmed = path.trimmed();
    if (trimmed.isEmpty()) {
        setLastError("Population derived metric export path cannot be empty");
        return false;
    }

    const QByteArray populationKeyUtf8 =
        (selectedPopulationKey_.isEmpty() ? QStringLiteral("__all__") : selectedPopulationKey_)
            .toUtf8();
    const QByteArray pathUtf8 = trimmed.toUtf8();
    return applyRustPayload(
        takeRustString(flowjoish_desktop_session_export_population_derived_metric_csv(
            session_,
            populationKeyUtf8.constData(),
            pathUtf8.constData())),
        true);
}

bool DesktopController::setActiveSampleGroupLabel(const QString &groupLabel) {
    if (session_ == nullptr) {
        setLastError("Desktop session is unavailable");
        return false;
    }

    const QString sampleId = activeSampleId();
    if (sampleId.isEmpty()) {
        setLastError("No active sample is available");
        return false;
    }

    const QByteArray sampleIdUtf8 = sampleId.toUtf8();
    const QByteArray groupLabelUtf8 = groupLabel.toUtf8();
    return applyRustPayload(
        takeRustString(flowjoish_desktop_session_set_sample_group_label(
            session_,
            sampleIdUtf8.constData(),
            groupLabelUtf8.constData())),
        true);
}

bool DesktopController::setDerivedMetricPositiveFraction(
    const QString &channel,
    double threshold) {
    const QString trimmedChannel = channel.trimmed();
    if (trimmedChannel.isEmpty()) {
        setLastError("Derived metric channel cannot be empty");
        return false;
    }
    if (!std::isfinite(threshold)) {
        setLastError("Derived metric threshold must be a finite number");
        return false;
    }

    QJsonObject metric;
    metric.insert("kind", "positive_fraction");
    metric.insert("channel", trimmedChannel);
    metric.insert("threshold", threshold);
    return setDerivedMetric(metric);
}

bool DesktopController::setDerivedMetricMeanRatio(
    const QString &numeratorChannel,
    const QString &denominatorChannel) {
    const QString trimmedNumerator = numeratorChannel.trimmed();
    const QString trimmedDenominator = denominatorChannel.trimmed();
    if (trimmedNumerator.isEmpty()) {
        setLastError("Derived metric numerator channel cannot be empty");
        return false;
    }
    if (trimmedDenominator.isEmpty()) {
        setLastError("Derived metric denominator channel cannot be empty");
        return false;
    }

    QJsonObject metric;
    metric.insert("kind", "mean_ratio");
    metric.insert("numerator_channel", trimmedNumerator);
    metric.insert("denominator_channel", trimmedDenominator);
    return setDerivedMetric(metric);
}

void DesktopController::loadWorkspace() {
    const QString path = QFileDialog::getOpenFileName(
        nullptr,
        tr("Load Workspace"),
        workspacePath_,
        tr("Parallax Workspace (*.parallax.json);;JSON Files (*.json);;All Files (*)"));
    if (!path.isEmpty()) {
        loadWorkspaceFile(path);
    }
}

bool DesktopController::loadWorkspaceFile(const QString &path) {
    if (session_ == nullptr) {
        setLastError("Desktop session is unavailable");
        return false;
    }

    const QString trimmed = path.trimmed();
    if (trimmed.isEmpty()) {
        setLastError("Workspace path cannot be empty");
        return false;
    }

    const QByteArray utf8 = trimmed.toUtf8();
    const bool loaded = applyRustPayload(
        takeRustString(
            flowjoish_desktop_session_load_workspace(session_, utf8.constData())),
        true);
    if (loaded) {
        setWorkspacePath(trimmed);
    }
    return loaded;
}

void DesktopController::setCompensationEnabled(bool enabled) {
    QJsonObject command;
    const QString sampleId = activeSampleId();
    command.insert(
        "kind",
        QStringLiteral("set_compensation_enabled"));
    command.insert(
        "sample_id",
        sampleId.isEmpty() ? QStringLiteral("desktop-demo") : sampleId);
    command.insert("enabled", enabled);
    dispatchCommandJson(QString::fromUtf8(
        QJsonDocument(command).toJson(QJsonDocument::Compact)));
}

void DesktopController::setChannelTransform(const QString &channel, const QString &kind) {
    if (channel.trimmed().isEmpty()) {
        setLastError("Transform channel cannot be empty");
        return;
    }

    QJsonObject transform;
    if (kind == "linear") {
        transform.insert("kind", "linear");
    } else if (kind == "signed_log10") {
        transform.insert("kind", "signed_log10");
    } else if (kind == "asinh") {
        transform.insert("kind", "asinh");
        transform.insert("cofactor", 150.0);
    } else if (kind == "biexponential") {
        transform.insert("kind", "biexponential");
        transform.insert("width_basis", 120.0);
        transform.insert("positive_decades", 4.5);
        transform.insert("negative_decades", 1.0);
    } else if (kind == "logicle") {
        transform.insert("kind", "logicle");
        transform.insert("decades", 4.5);
        transform.insert("linear_width", 12.0);
    } else {
        setLastError(QStringLiteral("Unknown transform '%1'").arg(kind));
        return;
    }

    QJsonObject command;
    const QString sampleId = activeSampleId();
    command.insert("kind", "set_channel_transform");
    command.insert(
        "sample_id",
        sampleId.isEmpty() ? QStringLiteral("desktop-demo") : sampleId);
    command.insert("channel", channel);
    command.insert("transform", transform);
    dispatchCommandJson(QString::fromUtf8(
        QJsonDocument(command).toJson(QJsonDocument::Compact)));
}

void DesktopController::resetPlotView(const QString &plotId) {
    if (plotId.trimmed().isEmpty()) {
        setLastError("Plot id cannot be empty");
        return;
    }

    QJsonObject command;
    const QString sampleId = activeSampleId();
    command.insert("kind", "reset_plot_view");
    command.insert(
        "sample_id",
        sampleId.isEmpty() ? QStringLiteral("desktop-demo") : sampleId);
    command.insert("plot_id", plotId);
    dispatchCommandJson(QString::fromUtf8(
        QJsonDocument(command).toJson(QJsonDocument::Compact)));
}

void DesktopController::focusPlotOnSelectedPopulation(const QString &plotId) {
    if (plotId.trimmed().isEmpty()) {
        setLastError("Plot id cannot be empty");
        return;
    }

    QJsonObject command;
    const QString sampleId = activeSampleId();
    command.insert("kind", "focus_plot_population");
    command.insert(
        "sample_id",
        sampleId.isEmpty() ? QStringLiteral("desktop-demo") : sampleId);
    command.insert("plot_id", plotId);
    command.insert(
        "population_id",
        selectedPopulationKey_.isEmpty() ? QStringLiteral("__all__") : selectedPopulationKey_);
    command.insert("padding_fraction", 0.08);
    dispatchCommandJson(QString::fromUtf8(
        QJsonDocument(command).toJson(QJsonDocument::Compact)));
}

void DesktopController::scalePlotView(const QString &plotId, double factor) {
    if (plotId.trimmed().isEmpty()) {
        setLastError("Plot id cannot be empty");
        return;
    }
    if (!std::isfinite(factor) || factor <= 0.0) {
        setLastError("Plot scale factor must be a positive finite number");
        return;
    }

    QJsonObject command;
    const QString sampleId = activeSampleId();
    command.insert("kind", "scale_plot_view");
    command.insert(
        "sample_id",
        sampleId.isEmpty() ? QStringLiteral("desktop-demo") : sampleId);
    command.insert("plot_id", plotId);
    command.insert("factor", factor);
    dispatchCommandJson(QString::fromUtf8(
        QJsonDocument(command).toJson(QJsonDocument::Compact)));
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
    const DesktopPayloadDecision decision = evaluateDesktopPayloadTransition(
        snapshot_,
        parsed,
        replaceSnapshotOnError);

    status_ = decision.status;
    setLastError(decision.errorMessage);
    if (decision.shouldReplaceSnapshot) {
        snapshot_ = parsed;
    }
    if (decision.shouldRebuildDerivedState) {
        rebuildDerivedState();
    }
    if (decision.shouldEmitSnapshotChanged) {
        emit snapshotChanged();
    }
    return decision.success;
}

void DesktopController::rebuildDerivedState() {
    sample_ = snapshot_.value("sample").toMap();
    samples_ = snapshot_.value("samples").toList();
    analysisActions_ = snapshot_.value("analysis_actions").toList();
    commandCount_ = snapshot_.value("command_count").toInt();
    canUndo_ = snapshot_.value("can_undo").toBool();
    canRedo_ = snapshot_.value("can_redo").toBool();
    commandLogHash_ = snapshot_.value("command_log_hash").toString();
    executionHash_ = snapshot_.value("execution_hash").toString();
    commands_ = snapshot_.value("commands").toList();
    populations_ = snapshot_.value("populations").toList();
    derivedMetric_ = snapshot_.value("derived_metric").toMap();

    QStringList populationKeys;
    for (const QVariant &value : populations_) {
        populationKeys.push_back(value.toMap().value("key").toString());
    }
    const QString nextSampleId = sample_.value("id").toString();
    if (nextSampleId != selectedSampleId_) {
        selectedSampleId_ = nextSampleId;
        emit selectedSampleIdChanged();
    }
    if (!populationKeys.contains(selectedPopulationKey_)) {
        selectedPopulationKey_ = populationKeys.contains("__all__") ? "__all__" : populationKeys.value(0);
        emit selectedPopulationKeyChanged();
    }
    selectedPopulationStats_ = snapshot_.value("population_stats")
                                   .toMap()
                                   .value(selectedPopulationKey_)
                                   .toMap();

    plots_.clear();
    for (const QVariant &value : snapshot_.value("plots").toList()) {
        QVariantMap plot = value.toMap();
        const QString kind = plot.value("kind").toString();
        if (kind == "histogram") {
            const QVariantMap populationBins = plot.value("population_bins").toMap();
            QVariantList highlightBins;
            if (selectedPopulationKey_ == "__all__") {
                highlightBins = plot.value("all_bins").toList();
            } else {
                highlightBins = populationBins.value(selectedPopulationKey_).toList();
            }
            int highlightCount = 0;
            for (const QVariant &binValue : highlightBins) {
                highlightCount += binValue.toMap().value("count").toInt();
            }
            plot.insert("highlight_bins", highlightBins);
            plot.insert("highlight_count", highlightCount);
        } else {
            const QVariantMap populationPoints = plot.value("population_points").toMap();
            const QVariantMap populationEventIndices =
                plot.value("population_event_indices").toMap();
            QVariantMap pointColumns = plot.value("point_columns").toMap();
            if (pointColumns.isEmpty()) {
                pointColumns = pointColumnsFromPointMaps(plot.value("all_points").toList());
            }
            QVariantMap highlightPointColumns;
            QVariantList highlightPoints;
            if (selectedPopulationKey_ == "__all__") {
                highlightPoints = plot.value("all_points").toList();
                highlightPointColumns = pointColumns;
            } else if (populationPoints.contains(selectedPopulationKey_)) {
                highlightPoints = populationPoints.value(selectedPopulationKey_).toList();
                highlightPointColumns = pointColumnsFromPointMaps(highlightPoints);
            } else {
                QSet<int> selectedEventIndices;
                for (const QVariant &indexValue :
                     populationEventIndices.value(selectedPopulationKey_).toList()) {
                    selectedEventIndices.insert(indexValue.toInt());
                }
                if (!pointColumns.isEmpty()) {
                    highlightPointColumns = filterPointColumns(pointColumns, selectedEventIndices);
                }
                if (highlightPointColumns.isEmpty()) {
                    for (const QVariant &pointValue : plot.value("all_points").toList()) {
                        const QVariantMap point = pointValue.toMap();
                        if (selectedEventIndices.contains(point.value("event_index").toInt())) {
                            highlightPoints.push_back(pointValue);
                        }
                    }
                    highlightPointColumns = pointColumnsFromPointMaps(highlightPoints);
                }
            }
            plot.insert("point_columns", pointColumns);
            plot.insert("highlight_point_columns", highlightPointColumns);
            plot.insert("highlight_points", highlightPoints);
            plot.insert("highlight_count", pointColumnCount(highlightPointColumns));
        }
        plots_.push_back(plot);
    }

    updateSelectedPopulationComparison();
}

void DesktopController::updateSelectedPopulationComparison() {
    const DesktopComparisonRefreshDecision decision =
        evaluateDesktopComparisonRefresh(
            snapshot_,
            selectedPopulationKey_,
            status_,
            selectedPopulationComparisonCacheKey_,
            pendingPopulationComparisonCacheKey_);

    if (decision.shouldClearComparison) {
        selectedPopulationComparison_.clear();
        selectedPopulationComparisonCacheKey_.clear();
        pendingPopulationComparisonCacheKey_.clear();
    }

    if (decision.shouldRequestRefresh) {
        queueSelectedPopulationComparisonRefresh(decision.cacheKey);
    }
}

void DesktopController::queueSelectedPopulationComparisonRefresh(const QString &cacheKey) {
    if (cacheKey.isEmpty()) {
        return;
    }

    pendingPopulationComparisonCacheKey_ = cacheKey;
    if (populationComparisonRefreshQueued_) {
        return;
    }

    populationComparisonRefreshQueued_ = true;
    QTimer::singleShot(0, this, [this]() {
        populationComparisonRefreshQueued_ = false;
        const QString cacheKey = pendingPopulationComparisonCacheKey_;
        if (!cacheKey.isEmpty()) {
            refreshSelectedPopulationComparison(cacheKey);
        }
    });
}

void DesktopController::refreshSelectedPopulationComparison(const QString &cacheKey) {
    if (cacheKey.isEmpty()
        || cacheKey != pendingPopulationComparisonCacheKey_
        || cacheKey != buildDesktopComparisonCacheKey(snapshot_, selectedPopulationKey_, status_)) {
        return;
    }

    if (session_ == nullptr || status_ != "ready") {
        return;
    }

    const QByteArray populationKeyUtf8 =
        (selectedPopulationKey_.isEmpty() ? QStringLiteral("__all__") : selectedPopulationKey_)
            .toUtf8();
    const QString payload = takeRustString(
        flowjoish_desktop_session_population_comparison_json(
            session_,
            populationKeyUtf8.constData()));
    if (payload.isEmpty()) {
        setLastError("Rust bridge returned an empty population comparison payload");
        return;
    }

    QJsonParseError parseError;
    const QJsonDocument document =
        QJsonDocument::fromJson(payload.toUtf8(), &parseError);
    if (parseError.error != QJsonParseError::NoError || !document.isObject()) {
        setLastError(
            QStringLiteral("Failed to parse population comparison payload: %1")
                .arg(parseError.errorString()));
        return;
    }

    const QVariantMap parsed = document.object().toVariantMap();
    if (parsed.value("status").toString() != "ready") {
        setLastError(parsed.value("message").toString());
        return;
    }

    if (cacheKey != pendingPopulationComparisonCacheKey_
        || cacheKey != buildDesktopComparisonCacheKey(snapshot_, selectedPopulationKey_, status_)) {
        return;
    }

    selectedPopulationComparison_ = parsed.value("population_comparison").toMap();
    selectedPopulationComparisonCacheKey_ = cacheKey;
    pendingPopulationComparisonCacheKey_.clear();
    emit snapshotChanged();
}

void DesktopController::setLastError(const QString &message) {
    if (message == lastError_) {
        return;
    }

    lastError_ = message;
    emit lastErrorChanged();
}

void DesktopController::setWorkspacePath(const QString &path) {
    if (path == workspacePath_) {
        return;
    }

    workspacePath_ = path;
    emit workspacePathChanged();
}

bool DesktopController::setDerivedMetric(const QJsonObject &metric) {
    if (session_ == nullptr) {
        setLastError("Desktop session is unavailable");
        return false;
    }

    const QByteArray payload =
        QJsonDocument(metric).toJson(QJsonDocument::Compact);
    return applyRustPayload(
        takeRustString(flowjoish_desktop_session_set_derived_metric_json(
            session_,
            payload.constData())),
        true);
}

QString DesktopController::buildPresetCommandJson(const QString &presetId) const {
    QJsonObject command;
    const QString sampleId = activeSampleId();
    command.insert(
        "sample_id",
        sampleId.isEmpty() ? QStringLiteral("desktop-demo") : sampleId);

    if (presetId == "lymphocytes") {
        const QString xChannel = findSampleChannel({"FSC-A", "FSC", "FSC-H"});
        const QString yChannel = findSampleChannel({"SSC-A", "SSC", "SSC-H"});
        if (xChannel.isEmpty() || yChannel.isEmpty()) {
            return QString();
        }

        command.insert("kind", "rectangle_gate");
        command.insert("population_id", "lymphocytes");
        command.insert("parent_population", QJsonValue());
        command.insert("x_channel", xChannel);
        command.insert("y_channel", yChannel);
        command.insert("x_min", 0.0);
        command.insert("x_max", 35.0);
        command.insert("y_min", 0.0);
        command.insert("y_max", 35.0);
    } else if (presetId == "cd3_cd4") {
        const QString xChannel = findSampleChannel({"CD3"});
        const QString yChannel = findSampleChannel({"CD4"});
        if (xChannel.isEmpty() || yChannel.isEmpty()) {
            return QString();
        }

        QJsonArray vertices;
        vertices.push_back(QJsonObject{{"x", 0.0}, {"y", 7.0}});
        vertices.push_back(QJsonObject{{"x", 6.0}, {"y", 7.0}});
        vertices.push_back(QJsonObject{{"x", 6.0}, {"y", 10.0}});
        vertices.push_back(QJsonObject{{"x", 0.0}, {"y", 10.0}});

        command.insert("kind", "polygon_gate");
        command.insert("population_id", "cd3_cd4");
        command.insert("parent_population", "lymphocytes");
        command.insert("x_channel", xChannel);
        command.insert("y_channel", yChannel);
        command.insert("vertices", vertices);
    } else {
        return QString();
    }

    return QString::fromUtf8(QJsonDocument(command).toJson(QJsonDocument::Compact));
}

bool DesktopController::presetIsAvailable(const QString &presetId) const {
    if (presetId == "lymphocytes") {
        return !findSampleChannel({"FSC-A", "FSC", "FSC-H"}).isEmpty()
            && !findSampleChannel({"SSC-A", "SSC", "SSC-H"}).isEmpty();
    }

    if (presetId == "cd3_cd4") {
        return hasPopulation("lymphocytes")
            && !findSampleChannel({"CD3"}).isEmpty()
            && !findSampleChannel({"CD4"}).isEmpty();
    }

    return false;
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

QString DesktopController::activeSampleId() const {
    const QString sampleId = sample_.value("id").toString();
    if (!sampleId.isEmpty()) {
        return sampleId;
    }
    return snapshot_.value("sample").toMap().value("id").toString();
}

QString DesktopController::findSampleChannel(const QStringList &candidates) const {
    const QVariantList channels = sample_.value("channels").toList();
    for (const QString &candidate : candidates) {
        for (const QVariant &value : channels) {
            if (value.toString().compare(candidate, Qt::CaseInsensitive) == 0) {
                return value.toString();
            }
        }
    }
    return QString();
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
