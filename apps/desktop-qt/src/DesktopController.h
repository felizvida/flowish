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
    Q_PROPERTY(QVariantList populations READ populations NOTIFY snapshotChanged)
    Q_PROPERTY(QVariantList commands READ commands NOTIFY snapshotChanged)
    Q_PROPERTY(QVariantList plots READ plots NOTIFY snapshotChanged)
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
    QVariantList populations() const;
    QVariantList commands() const;
    QVariantList plots() const;
    QString selectedPopulationKey() const;
    QString lastError() const;

    void setSelectedPopulationKey(const QString &populationKey);

    Q_INVOKABLE bool dispatchCommandJson(const QString &commandJson);
    Q_INVOKABLE void applyPresetCommand(const QString &presetId);
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
    void selectedPopulationKeyChanged();
    void lastErrorChanged();

private:
    bool applyRustPayload(const QString &payload, bool replaceSnapshotOnError = false);
    void rebuildDerivedState();
    void setLastError(const QString &message);
    QString buildPresetCommandJson(const QString &presetId) const;
    bool commitInteractiveCommand(const QJsonObject &command, const QString &populationId);
    QVariantMap plotDefinition(const QString &plotId) const;
    QString nextInteractivePopulationId(const QString &plotId) const;
    static QString sanitizePopulationSegment(const QString &value);

    void *session_ = nullptr;
    QVariantMap snapshot_;
    QVariantList populations_;
    QVariantList commands_;
    QVariantList plots_;
    QString status_ = "booting";
    int commandCount_ = 0;
    bool canUndo_ = false;
    bool canRedo_ = false;
    QString commandLogHash_;
    QString executionHash_;
    QString selectedPopulationKey_ = "__all__";
    QString lastError_;
};
