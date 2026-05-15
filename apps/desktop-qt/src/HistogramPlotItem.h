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

class HistogramPlotItem : public QQuickItem {
    Q_OBJECT
    Q_PROPERTY(QVariantList allBins READ allBins WRITE setAllBins NOTIFY allBinsChanged)
    Q_PROPERTY(QVariantList highlightBins READ highlightBins WRITE setHighlightBins NOTIFY highlightBinsChanged)
    Q_PROPERTY(QVariantList rangeOverlays READ rangeOverlays WRITE setRangeOverlays NOTIFY rangeOverlaysChanged)
    Q_PROPERTY(QString selectedPopulationKey READ selectedPopulationKey WRITE setSelectedPopulationKey NOTIFY selectedPopulationKeyChanged)
    Q_PROPERTY(double xMin READ xMin WRITE setXMin NOTIFY plotRangeChanged)
    Q_PROPERTY(double xMax READ xMax WRITE setXMax NOTIFY plotRangeChanged)
    Q_PROPERTY(double yMin READ yMin WRITE setYMin NOTIFY plotRangeChanged)
    Q_PROPERTY(double yMax READ yMax WRITE setYMax NOTIFY plotRangeChanged)

public:
    explicit HistogramPlotItem(QQuickItem *parent = nullptr);

    QVariantList allBins() const;
    QVariantList highlightBins() const;
    QVariantList rangeOverlays() const;
    QString selectedPopulationKey() const;
    double xMin() const;
    double xMax() const;
    double yMin() const;
    double yMax() const;

    void setAllBins(const QVariantList &bins);
    void setHighlightBins(const QVariantList &bins);
    void setRangeOverlays(const QVariantList &overlays);
    void setSelectedPopulationKey(const QString &populationKey);
    void setXMin(double value);
    void setXMax(double value);
    void setYMin(double value);
    void setYMax(double value);

signals:
    void allBinsChanged();
    void highlightBinsChanged();
    void rangeOverlaysChanged();
    void selectedPopulationKeyChanged();
    void plotRangeChanged();
    void rangeGateDrawn(double min, double max);

protected:
    QSGNode *updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *updatePaintNodeData) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;

private:
    struct RangeOverlay {
        QString populationId;
        double min;
        double max;
    };

    static QVector<QRectF> toBinRects(const QVariantList &values);
    static QVector<RangeOverlay> toRangeOverlays(const QVariantList &values);
    QRectF dataRect() const;
    QRectF plotRect() const;
    QRectF selectionRect() const;
    double mapPlotXToData(double x, const QRectF &bounds, const QRectF &plotArea) const;
    QRectF mapBinToPlot(const QRectF &dataRect, const QRectF &bounds, const QRectF &plotArea) const;
    QSGGeometryNode *buildSelectionNode(const QRectF &selectionRect) const;
    QSGGeometryNode *buildSelectionNode(const QRectF &selectionRect, const QColor &color) const;
    QSGGeometryNode *buildRangeOverlayNode(
        const RangeOverlay &overlay,
        const QColor &color,
        const QRectF &bounds,
        const QRectF &plotArea) const;
    QSGGeometryNode *buildBarsNode(
        const QVector<QRectF> &bins,
        const QColor &color,
        const QRectF &bounds,
        const QRectF &plotArea) const;

    QVariantList allBins_;
    QVariantList highlightBins_;
    QVariantList rangeOverlays_;
    QString selectedPopulationKey_ = "__all__";
    QVector<QRectF> allBinBuffer_;
    QVector<QRectF> highlightBinBuffer_;
    QVector<RangeOverlay> rangeOverlayBuffer_;
    double xMin_ = 0.0;
    double xMax_ = 1.0;
    double yMin_ = 0.0;
    double yMax_ = 1.0;
    bool dragging_ = false;
    QPointF dragStart_;
    QPointF dragCurrent_;
};
