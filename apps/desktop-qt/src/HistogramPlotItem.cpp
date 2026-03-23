#include "HistogramPlotItem.h"

#include <QColor>
#include <QSGFlatColorMaterial>
#include <QSGGeometry>
#include <QSGGeometryNode>
#include <QtMath>
#include <cmath>

namespace {

constexpr qreal kPlotPadding = 18.0;

bool sameRange(double left, double right) {
    return qAbs(left - right) < 1e-9;
}

}  // namespace

HistogramPlotItem::HistogramPlotItem(QQuickItem *parent) : QQuickItem(parent) {
    setFlag(ItemHasContents, true);
}

QVariantList HistogramPlotItem::allBins() const {
    return allBins_;
}

QVariantList HistogramPlotItem::highlightBins() const {
    return highlightBins_;
}

double HistogramPlotItem::xMin() const {
    return xMin_;
}

double HistogramPlotItem::xMax() const {
    return xMax_;
}

double HistogramPlotItem::yMin() const {
    return yMin_;
}

double HistogramPlotItem::yMax() const {
    return yMax_;
}

void HistogramPlotItem::setAllBins(const QVariantList &bins) {
    if (bins == allBins_) {
        return;
    }

    allBins_ = bins;
    allBinBuffer_ = toBinRects(bins);
    update();
    emit allBinsChanged();
}

void HistogramPlotItem::setHighlightBins(const QVariantList &bins) {
    if (bins == highlightBins_) {
        return;
    }

    highlightBins_ = bins;
    highlightBinBuffer_ = toBinRects(bins);
    update();
    emit highlightBinsChanged();
}

void HistogramPlotItem::setXMin(double value) {
    if (qFuzzyCompare(xMin_, value)) {
        return;
    }

    xMin_ = value;
    update();
    emit plotRangeChanged();
}

void HistogramPlotItem::setXMax(double value) {
    if (qFuzzyCompare(xMax_, value)) {
        return;
    }

    xMax_ = value;
    update();
    emit plotRangeChanged();
}

void HistogramPlotItem::setYMin(double value) {
    if (qFuzzyCompare(yMin_, value)) {
        return;
    }

    yMin_ = value;
    update();
    emit plotRangeChanged();
}

void HistogramPlotItem::setYMax(double value) {
    if (qFuzzyCompare(yMax_, value)) {
        return;
    }

    yMax_ = value;
    update();
    emit plotRangeChanged();
}

QSGNode *HistogramPlotItem::updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *) {
    auto *root = oldNode != nullptr ? oldNode : new QSGNode();
    while (QSGNode *child = root->firstChild()) {
        root->removeChildNode(child);
        delete child;
    }

    if (width() <= 0.0 || height() <= 0.0 || allBinBuffer_.isEmpty()) {
        return root;
    }

    const QRectF bounds = dataRect();
    const QRectF plotArea = plotRect();
    root->appendChildNode(buildBarsNode(allBinBuffer_, QColor("#365b63"), bounds, plotArea));
    if (!highlightBinBuffer_.isEmpty()) {
        root->appendChildNode(buildBarsNode(
            highlightBinBuffer_,
            QColor("#ef8354"),
            bounds,
            plotArea));
    }
    return root;
}

QVector<QRectF> HistogramPlotItem::toBinRects(const QVariantList &values) {
    QVector<QRectF> bins;
    bins.reserve(values.size());
    for (const QVariant &value : values) {
        const QVariantMap map = value.toMap();
        const double x0 = map.value("x0").toDouble();
        const double x1 = map.value("x1").toDouble();
        const double count = map.value("count").toDouble();
        if (!std::isfinite(x0) || !std::isfinite(x1) || !std::isfinite(count)) {
            continue;
        }
        bins.push_back(QRectF(x0, 0.0, x1 - x0, count));
    }
    return bins;
}

QRectF HistogramPlotItem::dataRect() const {
    double left = xMin_;
    double right = xMax_;
    double bottom = yMin_;
    double top = yMax_;

    if (!std::isfinite(left) || !std::isfinite(right) || sameRange(left, right)) {
        left = 0.0;
        right = 1.0;
    }
    if (!std::isfinite(bottom) || !std::isfinite(top) || sameRange(bottom, top)) {
        bottom = 0.0;
        top = 1.0;
    }

    return QRectF(left, bottom, right - left, top - bottom);
}

QRectF HistogramPlotItem::plotRect() const {
    return QRectF(
        kPlotPadding,
        kPlotPadding,
        qMax<qreal>(0.0, width() - (kPlotPadding * 2.0)),
        qMax<qreal>(0.0, height() - (kPlotPadding * 2.0)));
}

QRectF HistogramPlotItem::mapBinToPlot(
    const QRectF &dataRect,
    const QRectF &bounds,
    const QRectF &plotArea) const {
    const qreal xNormStart = (dataRect.left() - bounds.left()) / bounds.width();
    const qreal xNormEnd = (dataRect.right() - bounds.left()) / bounds.width();
    const qreal yNorm = (dataRect.height() - bounds.top()) / bounds.height();

    const qreal left = plotArea.left() + (xNormStart * plotArea.width());
    const qreal right = plotArea.left() + (xNormEnd * plotArea.width());
    const qreal top = plotArea.bottom() - (yNorm * plotArea.height());
    const qreal baseline = plotArea.bottom()
        - (((0.0 - bounds.top()) / bounds.height()) * plotArea.height());

    return QRectF(
        QPointF(qMin(left, right), qMin(top, baseline)),
        QPointF(qMax(left, right), qMax(top, baseline)));
}

QSGGeometryNode *HistogramPlotItem::buildBarsNode(
    const QVector<QRectF> &bins,
    const QColor &color,
    const QRectF &bounds,
    const QRectF &plotArea) const {
    auto *geometry = new QSGGeometry(QSGGeometry::defaultAttributes_Point2D(), bins.size() * 6);
    geometry->setDrawingMode(QSGGeometry::DrawTriangles);
    auto *vertices = geometry->vertexDataAsPoint2D();

    int vertexIndex = 0;
    for (const QRectF &bin : bins) {
        QRectF mapped = mapBinToPlot(bin, bounds, plotArea);
        if (mapped.width() < 1.0) {
            mapped.setWidth(1.0);
        }

        const float left = static_cast<float>(mapped.left());
        const float right = static_cast<float>(mapped.right());
        const float top = static_cast<float>(mapped.top());
        const float bottom = static_cast<float>(mapped.bottom());

        vertices[vertexIndex++].set(left, bottom);
        vertices[vertexIndex++].set(right, bottom);
        vertices[vertexIndex++].set(right, top);
        vertices[vertexIndex++].set(left, bottom);
        vertices[vertexIndex++].set(right, top);
        vertices[vertexIndex++].set(left, top);
    }

    auto *node = new QSGGeometryNode();
    node->setGeometry(geometry);
    node->setFlag(QSGNode::OwnsGeometry, true);

    auto *material = new QSGFlatColorMaterial();
    material->setColor(color);
    node->setMaterial(material);
    node->setFlag(QSGNode::OwnsMaterial, true);
    return node;
}
