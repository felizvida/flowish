#pragma once

#include <QObject>
#include <QString>
#include <QVariantList>
#include <QVariantMap>

class QJsonObject;

class DesktopController : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString status READ status NOTIFY snapshotChanged)
    Q_PROPERTY(int commandCount READ commandCount NOTIFY snapshotChanged)
    Q_PROPERTY(bool canUndo READ canUndo NOTIFY snapshotChanged)
    Q_PROPERTY(bool canRedo READ canRedo NOTIFY snapshotChanged)
    Q_PROPERTY(QString commandLogHash READ commandLogHash NOTIFY snapshotChanged)
    Q_PROPERTY(QString executionHash READ executionHash NOTIFY snapshotChanged)
    Q_PROPERTY(QString workspacePath READ workspacePath NOTIFY workspacePathChanged)
    Q_PROPERTY(QVariantMap sample READ sample NOTIFY snapshotChanged)
    Q_PROPERTY(QVariantList samples READ samples NOTIFY snapshotChanged)
    Q_PROPERTY(QVariantList analysisActions READ analysisActions NOTIFY snapshotChanged)
    Q_PROPERTY(QVariantList populations READ populations NOTIFY snapshotChanged)
    Q_PROPERTY(QVariantList commands READ commands NOTIFY snapshotChanged)
    Q_PROPERTY(QVariantList plots READ plots NOTIFY snapshotChanged)
    Q_PROPERTY(QVariantMap derivedMetric READ derivedMetric NOTIFY snapshotChanged)
    Q_PROPERTY(QVariantMap selectedPopulationStats READ selectedPopulationStats NOTIFY snapshotChanged)
    Q_PROPERTY(QVariantMap selectedPopulationComparison READ selectedPopulationComparison NOTIFY snapshotChanged)
    Q_PROPERTY(QString selectedSampleId READ selectedSampleId WRITE setSelectedSampleId NOTIFY selectedSampleIdChanged)
    Q_PROPERTY(QString selectedPopulationKey READ selectedPopulationKey WRITE setSelectedPopulationKey NOTIFY selectedPopulationKeyChanged)
    Q_PROPERTY(QString lastError READ lastError NOTIFY lastErrorChanged)

public:
    explicit DesktopController(QObject *parent = nullptr);
    ~DesktopController() override;

    QString status() const;
    int commandCount() const;
    bool canUndo() const;
    bool canRedo() const;
    QString commandLogHash() const;
    QString executionHash() const;
    QString workspacePath() const;
    QVariantMap sample() const;
    QVariantList samples() const;
    QVariantList analysisActions() const;
    QVariantList populations() const;
    QVariantList commands() const;
    QVariantList plots() const;
    QVariantMap derivedMetric() const;
    QVariantMap selectedPopulationStats() const;
    QVariantMap selectedPopulationComparison() const;
    QString selectedSampleId() const;
    QString selectedPopulationKey() const;
    QString lastError() const;

    void setSelectedSampleId(const QString &sampleId);
    void setSelectedPopulationKey(const QString &populationKey);

    Q_INVOKABLE bool dispatchCommandJson(const QString &commandJson);
    Q_INVOKABLE void applyPresetCommand(const QString &presetId);
    Q_INVOKABLE bool canApplyPreset(const QString &presetId) const;
    Q_INVOKABLE void importFcsFiles();
    Q_INVOKABLE bool loadSampleFiles(const QStringList &paths);
    Q_INVOKABLE void saveWorkspaceAs();
    Q_INVOKABLE bool saveWorkspaceToFile(const QString &path);
    Q_INVOKABLE void exportStatsCsv();
    Q_INVOKABLE bool exportStatsCsvToFile(const QString &path);
    Q_INVOKABLE void applyActiveTemplateToOtherSamples();
    Q_INVOKABLE void exportBatchStatsCsv();
    Q_INVOKABLE bool exportBatchStatsCsvToFile(const QString &path);
    Q_INVOKABLE void exportSelectedPopulationComparisonCsv();
    Q_INVOKABLE bool exportSelectedPopulationComparisonCsvToFile(const QString &path);
    Q_INVOKABLE void exportSelectedPopulationGroupSummaryCsv();
    Q_INVOKABLE bool exportSelectedPopulationGroupSummaryCsvToFile(const QString &path);
    Q_INVOKABLE void exportSelectedPopulationDerivedMetricCsv();
    Q_INVOKABLE bool exportSelectedPopulationDerivedMetricCsvToFile(const QString &path);
    Q_INVOKABLE void loadWorkspace();
    Q_INVOKABLE bool loadWorkspaceFile(const QString &path);
    Q_INVOKABLE bool setActiveSampleGroupLabel(const QString &groupLabel);
    Q_INVOKABLE bool setDerivedMetricPositiveFraction(const QString &channel, double threshold);
    Q_INVOKABLE bool setDerivedMetricMeanRatio(
        const QString &numeratorChannel,
        const QString &denominatorChannel);
    Q_INVOKABLE void setCompensationEnabled(bool enabled);
    Q_INVOKABLE void setChannelTransform(const QString &channel, const QString &kind);
    Q_INVOKABLE void resetPlotView(const QString &plotId);
    Q_INVOKABLE void focusPlotOnSelectedPopulation(const QString &plotId);
    Q_INVOKABLE void scalePlotView(const QString &plotId, double factor);
    Q_INVOKABLE void resetSession();
    Q_INVOKABLE void undo();
    Q_INVOKABLE void redo();
    Q_INVOKABLE bool hasPopulation(const QString &populationKey) const;
    Q_INVOKABLE bool createRectangleGateForPlot(
        const QString &plotId,
        double xMin,
        double xMax,
        double yMin,
        double yMax);
    Q_INVOKABLE bool createPolygonGateForPlot(
        const QString &plotId,
        const QVariantList &vertices);

signals:
    void snapshotChanged();
    void workspacePathChanged();
    void selectedSampleIdChanged();
    void selectedPopulationKeyChanged();
    void lastErrorChanged();

private:
    bool applyRustPayload(const QString &payload, bool replaceSnapshotOnError = false);
    void rebuildDerivedState();
    void refreshSelectedPopulationComparison();
    void setLastError(const QString &message);
    void setWorkspacePath(const QString &path);
    bool setDerivedMetric(const QJsonObject &metric);
    QString buildPresetCommandJson(const QString &presetId) const;
    bool presetIsAvailable(const QString &presetId) const;
    bool commitInteractiveCommand(const QJsonObject &command, const QString &populationId);
    QVariantMap plotDefinition(const QString &plotId) const;
    QString nextInteractivePopulationId(const QString &plotId) const;
    QString activeSampleId() const;
    QString findSampleChannel(const QStringList &candidates) const;
    static QString sanitizePopulationSegment(const QString &value);

    void *session_ = nullptr;
    QVariantMap snapshot_;
    QVariantMap sample_;
    QVariantList samples_;
    QVariantList analysisActions_;
    QVariantList populations_;
    QVariantList commands_;
    QVariantList plots_;
    QVariantMap derivedMetric_;
    QVariantMap selectedPopulationStats_;
    QVariantMap selectedPopulationComparison_;
    QString status_ = "booting";
    int commandCount_ = 0;
    bool canUndo_ = false;
    bool canRedo_ = false;
    QString commandLogHash_;
    QString executionHash_;
    QString workspacePath_;
    QString selectedSampleId_;
    QString selectedPopulationKey_ = "__all__";
    QString lastError_;
};
