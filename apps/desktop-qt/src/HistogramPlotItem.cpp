#include "HistogramPlotItem.h"

#include <QColor>
#include <QMouseEvent>
#include <QSGFlatColorMaterial>
#include <QSGGeometry>
#include <QSGGeometryNode>
#include <QtMath>
#include <cmath>

namespace {

constexpr qreal kPlotPadding = 18.0;
constexpr qreal kMinimumDragPixels = 8.0;

bool sameRange(double left, double right) {
    return qAbs(left - right) < 1e-9;
}

}  // namespace

HistogramPlotItem::HistogramPlotItem(QQuickItem *parent) : QQuickItem(parent) {
    setFlag(ItemHasContents, true);
    setAcceptedMouseButtons(Qt::LeftButton);
}

QVariantList HistogramPlotItem::allBins() const {
    return allBins_;
}

QVariantList HistogramPlotItem::highlightBins() const {
    return highlightBins_;
}

QVariantList HistogramPlotItem::rangeOverlays() const {
    return rangeOverlays_;
}

QString HistogramPlotItem::selectedPopulationKey() const {
    return selectedPopulationKey_;
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

void HistogramPlotItem::setRangeOverlays(const QVariantList &overlays) {
    if (overlays == rangeOverlays_) {
        return;
    }

    rangeOverlays_ = overlays;
    rangeOverlayBuffer_ = toRangeOverlays(overlays);
    update();
    emit rangeOverlaysChanged();
}

void HistogramPlotItem::setSelectedPopulationKey(const QString &populationKey) {
    const QString nextKey = populationKey.trimmed().isEmpty() ? QStringLiteral("__all__") : populationKey;
    if (selectedPopulationKey_ == nextKey) {
        return;
    }

    selectedPopulationKey_ = nextKey;
    update();
    emit selectedPopulationKeyChanged();
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
    for (const RangeOverlay &overlay : rangeOverlayBuffer_) {
        if (overlay.populationId != selectedPopulationKey_) {
            QColor color("#6f8b8f");
            color.setAlphaF(0.16);
            root->appendChildNode(buildRangeOverlayNode(overlay, color, bounds, plotArea));
        }
    }
    if (!highlightBinBuffer_.isEmpty()) {
        root->appendChildNode(buildBarsNode(
            highlightBinBuffer_,
            QColor("#ef8354"),
            bounds,
            plotArea));
    }
    for (const RangeOverlay &overlay : rangeOverlayBuffer_) {
        if (overlay.populationId == selectedPopulationKey_) {
            QColor color("#ef8354");
            color.setAlphaF(0.24);
            root->appendChildNode(buildRangeOverlayNode(overlay, color, bounds, plotArea));
        }
    }
    if (dragging_) {
        const QRectF activeSelection = selectionRect();
        if (activeSelection.width() >= 1.0) {
            root->appendChildNode(buildSelectionNode(activeSelection));
        }
    }
    return root;
}

void HistogramPlotItem::mousePressEvent(QMouseEvent *event) {
    if (event->button() != Qt::LeftButton) {
        QQuickItem::mousePressEvent(event);
        return;
    }

    dragging_ = true;
    dragStart_ = event->localPos();
    dragCurrent_ = dragStart_;
    update();
    event->accept();
}

void HistogramPlotItem::mouseMoveEvent(QMouseEvent *event) {
    if (!dragging_) {
        QQuickItem::mouseMoveEvent(event);
        return;
    }

    dragCurrent_ = event->localPos();
    update();
    event->accept();
}

void HistogramPlotItem::mouseReleaseEvent(QMouseEvent *event) {
    if (!dragging_ || event->button() != Qt::LeftButton) {
        QQuickItem::mouseReleaseEvent(event);
        return;
    }

    dragCurrent_ = event->localPos();
    const QRectF activeSelection = selectionRect();
    const bool validSelection = activeSelection.width() >= kMinimumDragPixels;
    const QRectF bounds = dataRect();
    const QRectF plotArea = plotRect();

    dragging_ = false;
    update();

    if (validSelection) {
        emit rangeGateDrawn(
            mapPlotXToData(activeSelection.left(), bounds, plotArea),
            mapPlotXToData(activeSelection.right(), bounds, plotArea));
    }

    event->accept();
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

QVector<HistogramPlotItem::RangeOverlay> HistogramPlotItem::toRangeOverlays(const QVariantList &values) {
    QVector<RangeOverlay> overlays;
    overlays.reserve(values.size());
    for (const QVariant &value : values) {
        const QVariantMap map = value.toMap();
        bool okMin = false;
        bool okMax = false;
        const double min = map.value("min").toDouble(&okMin);
        const double max = map.value("max").toDouble(&okMax);
        if (!okMin || !okMax || !std::isfinite(min) || !std::isfinite(max) || sameRange(min, max)) {
            continue;
        }

        overlays.push_back(RangeOverlay {
            map.value("population_id").toString(),
            qMin(min, max),
            qMax(min, max),
        });
    }
    return overlays;
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
        qMax<qreal>(1.0, width() - (kPlotPadding * 2.0)),
        qMax<qreal>(1.0, height() - (kPlotPadding * 2.0)));
}

QRectF HistogramPlotItem::selectionRect() const {
    const QRectF plotArea = plotRect();
    const qreal left = qBound(plotArea.left(), qMin(dragStart_.x(), dragCurrent_.x()), plotArea.right());
    const qreal right = qBound(plotArea.left(), qMax(dragStart_.x(), dragCurrent_.x()), plotArea.right());
    return QRectF(QPointF(left, plotArea.top()), QPointF(right, plotArea.bottom())).normalized();
}

double HistogramPlotItem::mapPlotXToData(
    double x,
    const QRectF &bounds,
    const QRectF &plotArea) const {
    const qreal clampedX = qBound(plotArea.left(), x, plotArea.right());
    const qreal xNorm = (clampedX - plotArea.left()) / plotArea.width();
    return bounds.left() + (xNorm * bounds.width());
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

QSGGeometryNode *HistogramPlotItem::buildSelectionNode(const QRectF &selectionRect) const {
    QColor selectionFill("#ef8354");
    selectionFill.setAlphaF(0.24);
    return buildSelectionNode(selectionRect, selectionFill);
}

QSGGeometryNode *HistogramPlotItem::buildSelectionNode(
    const QRectF &selectionRect,
    const QColor &color) const {
    auto *geometry = new QSGGeometry(QSGGeometry::defaultAttributes_Point2D(), 6);
    geometry->setDrawingMode(QSGGeometry::DrawTriangles);
    auto *vertices = geometry->vertexDataAsPoint2D();

    const float left = static_cast<float>(selectionRect.left());
    const float right = static_cast<float>(selectionRect.right());
    const float top = static_cast<float>(selectionRect.top());
    const float bottom = static_cast<float>(selectionRect.bottom());

    vertices[0].set(left, top);
    vertices[1].set(right, top);
    vertices[2].set(left, bottom);
    vertices[3].set(left, bottom);
    vertices[4].set(right, top);
    vertices[5].set(right, bottom);

    auto *material = new QSGFlatColorMaterial();
    material->setColor(color);

    auto *node = new QSGGeometryNode();
    node->setGeometry(geometry);
    node->setMaterial(material);
    node->setFlag(QSGNode::OwnsGeometry, true);
    node->setFlag(QSGNode::OwnsMaterial, true);
    return node;
}

QSGGeometryNode *HistogramPlotItem::buildRangeOverlayNode(
    const RangeOverlay &overlay,
    const QColor &color,
    const QRectF &bounds,
    const QRectF &plotArea) const {
    const qreal xNormStart = (overlay.min - bounds.left()) / bounds.width();
    const qreal xNormEnd = (overlay.max - bounds.left()) / bounds.width();
    const qreal rawLeft = plotArea.left() + (xNormStart * plotArea.width());
    const qreal rawRight = plotArea.left() + (xNormEnd * plotArea.width());
    const qreal left = qBound(plotArea.left(), qMin(rawLeft, rawRight), plotArea.right());
    const qreal right = qBound(plotArea.left(), qMax(rawLeft, rawRight), plotArea.right());
    QRectF mapped(QPointF(left, plotArea.top()), QPointF(right, plotArea.bottom()));
    if (mapped.width() < 1.0) {
        mapped.setWidth(1.0);
    }
    return buildSelectionNode(mapped.normalized(), color);
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
