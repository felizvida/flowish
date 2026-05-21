#pragma once

#include <QPointF>
#include <QQuickItem>
#include <QRectF>
#include <QString>
#include <QVariantList>
#include <QVariantMap>
#include <QVector>

class QColor;
class QMouseEvent;
class QSGGeometryNode;
class QSGNode;

class ScatterPlotItem : public QQuickItem {
    Q_OBJECT
    Q_PROPERTY(QVariantList allPoints READ allPoints WRITE setAllPoints NOTIFY allPointsChanged)
    Q_PROPERTY(QVariantList highlightPoints READ highlightPoints WRITE setHighlightPoints NOTIFY highlightPointsChanged)
    Q_PROPERTY(QVariantMap pointColumns READ pointColumns WRITE setPointColumns NOTIFY pointColumnsChanged)
    Q_PROPERTY(QVariantMap highlightPointColumns READ highlightPointColumns WRITE setHighlightPointColumns NOTIFY highlightPointColumnsChanged)
    Q_PROPERTY(QVariantMap densityGrid READ densityGrid WRITE setDensityGrid NOTIFY densityGridChanged)
    Q_PROPERTY(QVariantList gateOverlays READ gateOverlays WRITE setGateOverlays NOTIFY gateOverlaysChanged)
    Q_PROPERTY(QString selectedPopulationKey READ selectedPopulationKey WRITE setSelectedPopulationKey NOTIFY selectedPopulationKeyChanged)
    Q_PROPERTY(double xMin READ xMin WRITE setXMin NOTIFY plotRangeChanged)
    Q_PROPERTY(double xMax READ xMax WRITE setXMax NOTIFY plotRangeChanged)
    Q_PROPERTY(double yMin READ yMin WRITE setYMin NOTIFY plotRangeChanged)
    Q_PROPERTY(double yMax READ yMax WRITE setYMax NOTIFY plotRangeChanged)
    Q_PROPERTY(QString interactionMode READ interactionMode WRITE setInteractionMode NOTIFY interactionModeChanged)

public:
    explicit ScatterPlotItem(QQuickItem *parent = nullptr);

    QVariantList allPoints() const;
    QVariantList highlightPoints() const;
    QVariantMap pointColumns() const;
    QVariantMap highlightPointColumns() const;
    QVariantMap densityGrid() const;
    QVariantList gateOverlays() const;
    QString selectedPopulationKey() const;
    double xMin() const;
    double xMax() const;
    double yMin() const;
    double yMax() const;
    QString interactionMode() const;

    void setAllPoints(const QVariantList &points);
    void setHighlightPoints(const QVariantList &points);
    void setPointColumns(const QVariantMap &columns);
    void setHighlightPointColumns(const QVariantMap &columns);
    void setDensityGrid(const QVariantMap &densityGrid);
    void setGateOverlays(const QVariantList &overlays);
    void setSelectedPopulationKey(const QString &populationKey);
    void setXMin(double value);
    void setXMax(double value);
    void setYMin(double value);
    void setYMax(double value);
    void setInteractionMode(const QString &mode);

signals:
    void allPointsChanged();
    void highlightPointsChanged();
    void pointColumnsChanged();
    void highlightPointColumnsChanged();
    void densityGridChanged();
    void gateOverlaysChanged();
    void selectedPopulationKeyChanged();
    void plotRangeChanged();
    void interactionModeChanged();
    void rectangleGateDrawn(double xMin, double xMax, double yMin, double yMax);
    void rectangleGateEdited(const QString &populationId, double xMin, double xMax, double yMin, double yMax);
    void polygonGateDrawn(const QVariantList &vertices);
    void polygonGateEdited(const QString &populationId, const QVariantList &vertices);
    void plotPanned(double xDelta, double yDelta);

protected:
    QSGNode *updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *updatePaintNodeData) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;

private:
    struct GateOverlay {
        QString kind;
        QString populationId;
        QVector<QPointF> vertices;
    };

    enum class RectangleEditHandle {
        None,
        Move,
        Left,
        Right,
        Top,
        Bottom,
        TopLeft,
        TopRight,
        BottomLeft,
        BottomRight,
    };

    struct RectangleEditState {
        bool active = false;
        QString populationId;
        QRectF startBounds;
        QPointF startData;
        RectangleEditHandle handle = RectangleEditHandle::None;
    };

    enum class PolygonEditHandle {
        None,
        Move,
        Vertex,
    };

    struct PolygonEditHit {
        PolygonEditHandle handle = PolygonEditHandle::None;
        int vertexIndex = -1;
    };

    struct PolygonEditState {
        bool active = false;
        QString populationId;
        QVector<QPointF> startVertices;
        QPointF startData;
        PolygonEditHandle handle = PolygonEditHandle::None;
        int vertexIndex = -1;
    };

    struct DensityCell {
        QRectF bounds;
        double intensity;
    };

    static QVector<QPointF> toPointVector(const QVariantList &values);
    static QVector<QPointF> toPointVector(const QVariantMap &columns);
    static QVector<DensityCell> toDensityCells(const QVariantMap &densityGrid);
    static QVector<GateOverlay> toGateOverlays(const QVariantList &values);
    static QVariantList toVariantList(const QVector<QPointF> &values);
    QRectF dataRect() const;
    QRectF plotRect() const;
    QRectF selectionRect() const;
    QPointF mapDataToPlot(const QPointF &point, const QRectF &bounds, const QRectF &plotArea) const;
    QPointF mapPlotToData(const QPointF &point, const QRectF &bounds, const QRectF &plotArea) const;
    bool isPolygonMode() const;
    bool isPanMode() const;
    bool isEditMode() const;
    void clearInteractionDraft();
    const GateOverlay *selectedRectangleOverlay() const;
    const GateOverlay *selectedPolygonOverlay() const;
    QRectF overlayBoundsData(const GateOverlay &overlay) const;
    QRectF mapDataBoundsToPlot(const QRectF &dataBounds, const QRectF &bounds, const QRectF &plotArea) const;
    RectangleEditHandle hitTestRectangleEdit(
        const GateOverlay &overlay,
        const QPointF &plotPosition,
        const QRectF &bounds,
        const QRectF &plotArea) const;
    bool beginRectangleEdit(const QPointF &plotPosition);
    QRectF editedRectangleBounds(const QPointF &plotPosition) const;
    QVector<QPointF> rectangleVertices(const QRectF &dataBounds) const;
    QVector<QPointF> editablePolygonVertices(const GateOverlay &overlay) const;
    QVector<QPointF> closedPolygonPath(const QVector<QPointF> &vertices) const;
    PolygonEditHit hitTestPolygonEdit(
        const GateOverlay &overlay,
        const QPointF &plotPosition,
        const QRectF &bounds,
        const QRectF &plotArea) const;
    bool polygonContainsPlotPoint(
        const QVector<QPointF> &vertices,
        const QPointF &plotPosition,
        const QRectF &bounds,
        const QRectF &plotArea) const;
    bool beginPolygonEdit(const QPointF &plotPosition);
    QVector<QPointF> editedPolygonVertices(const QPointF &plotPosition) const;
    bool polygonVerticesChanged(const QVector<QPointF> &left, const QVector<QPointF> &right) const;
    QSGGeometryNode *buildSeriesNode(
        const QVector<QPointF> &points,
        const QColor &color,
        qreal pointSize,
        const QRectF &dataRect,
        const QRectF &plotArea) const;
    QSGGeometryNode *buildDensityNode(
        const QVector<DensityCell> &cells,
        int band,
        int bandCount,
        const QRectF &dataRect,
        const QRectF &plotArea) const;
    QSGGeometryNode *buildSelectionNode(const QRectF &selectionRect) const;
    QSGGeometryNode *buildPolylineNode(
        const QVector<QPointF> &points,
        const QColor &color,
        const QRectF &dataRect,
        const QRectF &plotArea) const;

    QVariantList allPoints_;
    QVariantList highlightPoints_;
    QVariantMap pointColumns_;
    QVariantMap highlightPointColumns_;
    QVariantMap densityGrid_;
    QVariantList gateOverlays_;
    QString selectedPopulationKey_ = "__all__";
    QVector<QPointF> allPointBuffer_;
    QVector<QPointF> highlightPointBuffer_;
    QVector<DensityCell> densityCellBuffer_;
    QVector<GateOverlay> gateOverlayBuffer_;
    double xMin_ = 0.0;
    double xMax_ = 1.0;
    double yMin_ = 0.0;
    double yMax_ = 1.0;
    QString interactionMode_ = "rectangle";
    bool dragging_ = false;
    QPointF dragStart_;
    QPointF dragCurrent_;
    RectangleEditState rectangleEdit_;
    PolygonEditState polygonEdit_;
    QVector<QPointF> polygonVertices_;
    QPointF polygonHover_;
    bool polygonHasHover_ = false;
};
