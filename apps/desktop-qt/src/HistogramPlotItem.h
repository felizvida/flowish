#pragma once

#include <QQuickItem>
#include <QRectF>
#include <QVariantList>
#include <QVector>

class QColor;
class QSGGeometryNode;
class QSGNode;

class HistogramPlotItem : public QQuickItem {
    Q_OBJECT
    Q_PROPERTY(QVariantList allBins READ allBins WRITE setAllBins NOTIFY allBinsChanged)
    Q_PROPERTY(QVariantList highlightBins READ highlightBins WRITE setHighlightBins NOTIFY highlightBinsChanged)
    Q_PROPERTY(double xMin READ xMin WRITE setXMin NOTIFY plotRangeChanged)
    Q_PROPERTY(double xMax READ xMax WRITE setXMax NOTIFY plotRangeChanged)
    Q_PROPERTY(double yMin READ yMin WRITE setYMin NOTIFY plotRangeChanged)
    Q_PROPERTY(double yMax READ yMax WRITE setYMax NOTIFY plotRangeChanged)

public:
    explicit HistogramPlotItem(QQuickItem *parent = nullptr);

    QVariantList allBins() const;
    QVariantList highlightBins() const;
    double xMin() const;
    double xMax() const;
    double yMin() const;
    double yMax() const;

    void setAllBins(const QVariantList &bins);
    void setHighlightBins(const QVariantList &bins);
    void setXMin(double value);
    void setXMax(double value);
    void setYMin(double value);
    void setYMax(double value);

signals:
    void allBinsChanged();
    void highlightBinsChanged();
    void plotRangeChanged();

protected:
    QSGNode *updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *updatePaintNodeData) override;

private:
    static QVector<QRectF> toBinRects(const QVariantList &values);
    QRectF dataRect() const;
    QRectF plotRect() const;
    QRectF mapBinToPlot(const QRectF &dataRect, const QRectF &bounds, const QRectF &plotArea) const;
    QSGGeometryNode *buildBarsNode(
        const QVector<QRectF> &bins,
        const QColor &color,
        const QRectF &bounds,
        const QRectF &plotArea) const;

    QVariantList allBins_;
    QVariantList highlightBins_;
    QVector<QRectF> allBinBuffer_;
    QVector<QRectF> highlightBinBuffer_;
    double xMin_ = 0.0;
    double xMax_ = 1.0;
    double yMin_ = 0.0;
    double yMax_ = 1.0;
};
