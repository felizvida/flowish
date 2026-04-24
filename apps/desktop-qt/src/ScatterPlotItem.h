#pragma once

#include <QPointF>
#include <QQuickItem>
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
    double xMin() const;
    double xMax() const;
    double yMin() const;
    double yMax() const;
    QString interactionMode() const;

    void setAllPoints(const QVariantList &points);
    void setHighlightPoints(const QVariantList &points);
    void setPointColumns(const QVariantMap &columns);
    void setHighlightPointColumns(const QVariantMap &columns);
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
    void plotRangeChanged();
    void interactionModeChanged();
    void rectangleGateDrawn(double xMin, double xMax, double yMin, double yMax);
    void polygonGateDrawn(const QVariantList &vertices);

protected:
    QSGNode *updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *updatePaintNodeData) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;

private:
    static QVector<QPointF> toPointVector(const QVariantList &values);
    static QVector<QPointF> toPointVector(const QVariantMap &columns);
    static QVariantList toVariantList(const QVector<QPointF> &values);
    QRectF dataRect() const;
    QRectF plotRect() const;
    QRectF selectionRect() const;
    QPointF mapDataToPlot(const QPointF &point, const QRectF &bounds, const QRectF &plotArea) const;
    QPointF mapPlotToData(const QPointF &point, const QRectF &bounds, const QRectF &plotArea) const;
    bool isPolygonMode() const;
    void clearInteractionDraft();
    QSGGeometryNode *buildSeriesNode(
        const QVector<QPointF> &points,
        const QColor &color,
        qreal pointSize,
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
    QVector<QPointF> allPointBuffer_;
    QVector<QPointF> highlightPointBuffer_;
    double xMin_ = 0.0;
    double xMax_ = 1.0;
    double yMin_ = 0.0;
    double yMax_ = 1.0;
    QString interactionMode_ = "rectangle";
    bool dragging_ = false;
    QPointF dragStart_;
    QPointF dragCurrent_;
    QVector<QPointF> polygonVertices_;
    QPointF polygonHover_;
    bool polygonHasHover_ = false;
};
