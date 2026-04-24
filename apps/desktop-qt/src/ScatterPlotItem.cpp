#include "ScatterPlotItem.h"

#include <QColor>
#include <QMouseEvent>
#include <QRectF>
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

ScatterPlotItem::ScatterPlotItem(QQuickItem *parent) : QQuickItem(parent) {
    setFlag(ItemHasContents, true);
    setAcceptedMouseButtons(Qt::LeftButton | Qt::RightButton);
}

QVariantList ScatterPlotItem::allPoints() const {
    return allPoints_;
}

QVariantList ScatterPlotItem::highlightPoints() const {
    return highlightPoints_;
}

QVariantMap ScatterPlotItem::pointColumns() const {
    return pointColumns_;
}

QVariantMap ScatterPlotItem::highlightPointColumns() const {
    return highlightPointColumns_;
}

double ScatterPlotItem::xMin() const {
    return xMin_;
}

double ScatterPlotItem::xMax() const {
    return xMax_;
}

double ScatterPlotItem::yMin() const {
    return yMin_;
}

double ScatterPlotItem::yMax() const {
    return yMax_;
}

QString ScatterPlotItem::interactionMode() const {
    return interactionMode_;
}

void ScatterPlotItem::setAllPoints(const QVariantList &points) {
    if (points == allPoints_) {
        return;
    }

    allPoints_ = points;
    allPointBuffer_ = toPointVector(points);
    update();
    emit allPointsChanged();
}

void ScatterPlotItem::setHighlightPoints(const QVariantList &points) {
    if (points == highlightPoints_) {
        return;
    }

    highlightPoints_ = points;
    highlightPointBuffer_ = toPointVector(points);
    update();
    emit highlightPointsChanged();
}

void ScatterPlotItem::setPointColumns(const QVariantMap &columns) {
    if (columns == pointColumns_) {
        return;
    }

    pointColumns_ = columns;
    allPointBuffer_ = toPointVector(columns);
    update();
    emit pointColumnsChanged();
}

void ScatterPlotItem::setHighlightPointColumns(const QVariantMap &columns) {
    if (columns == highlightPointColumns_) {
        return;
    }

    highlightPointColumns_ = columns;
    highlightPointBuffer_ = toPointVector(columns);
    update();
    emit highlightPointColumnsChanged();
}

void ScatterPlotItem::setXMin(double value) {
    if (qFuzzyCompare(xMin_, value)) {
        return;
    }

    xMin_ = value;
    update();
    emit plotRangeChanged();
}

void ScatterPlotItem::setXMax(double value) {
    if (qFuzzyCompare(xMax_, value)) {
        return;
    }

    xMax_ = value;
    update();
    emit plotRangeChanged();
}

void ScatterPlotItem::setYMin(double value) {
    if (qFuzzyCompare(yMin_, value)) {
        return;
    }

    yMin_ = value;
    update();
    emit plotRangeChanged();
}

void ScatterPlotItem::setYMax(double value) {
    if (qFuzzyCompare(yMax_, value)) {
        return;
    }

    yMax_ = value;
    update();
    emit plotRangeChanged();
}

void ScatterPlotItem::setInteractionMode(const QString &mode) {
    const QString normalized = mode.trimmed().toLower();
    const QString nextMode = normalized == "polygon" ? QStringLiteral("polygon") : QStringLiteral("rectangle");
    if (interactionMode_ == nextMode) {
        return;
    }

    interactionMode_ = nextMode;
    clearInteractionDraft();
    emit interactionModeChanged();
}

QSGNode *ScatterPlotItem::updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *) {
    auto *root = oldNode != nullptr ? oldNode : new QSGNode();
    while (QSGNode *child = root->firstChild()) {
        root->removeChildNode(child);
        delete child;
    }

    if (width() <= 0.0 || height() <= 0.0 || allPointBuffer_.isEmpty()) {
        return root;
    }

    const QRectF bounds = dataRect();
    const QRectF plotArea = plotRect();
    root->appendChildNode(buildSeriesNode(allPointBuffer_, QColor("#365b63"), 6.0, bounds, plotArea));
    if (!highlightPointBuffer_.isEmpty()) {
        root->appendChildNode(buildSeriesNode(
            highlightPointBuffer_,
            QColor("#ef8354"),
            10.0,
            bounds,
            plotArea));
    }
    if (dragging_) {
        const QRectF activeSelection = selectionRect();
        if (activeSelection.width() >= 1.0 && activeSelection.height() >= 1.0) {
            root->appendChildNode(buildSelectionNode(activeSelection));
        }
    }
    if (!polygonVertices_.isEmpty()) {
        QVector<QPointF> committedPath = polygonVertices_;
        if (committedPath.size() >= 2) {
            root->appendChildNode(buildPolylineNode(committedPath, QColor("#9a7b3f"), bounds, plotArea));
        }

        QVector<QPointF> previewPath = polygonVertices_;
        if (polygonHasHover_) {
            previewPath.push_back(polygonHover_);
        }
        if (previewPath.size() >= 2) {
            root->appendChildNode(buildPolylineNode(previewPath, QColor("#ef8354"), bounds, plotArea));
        }
        root->appendChildNode(buildSeriesNode(polygonVertices_, QColor("#ef8354"), 11.0, bounds, plotArea));
    }
    return root;
}

void ScatterPlotItem::mousePressEvent(QMouseEvent *event) {
    const QRectF plotArea = plotRect();
    const QRectF bounds = dataRect();
    if (isPolygonMode()) {
        if (event->button() == Qt::LeftButton) {
            polygonVertices_.push_back(mapPlotToData(event->localPos(), bounds, plotArea));
            polygonHover_ = polygonVertices_.last();
            polygonHasHover_ = true;
            update();
            event->accept();
            return;
        }

        if (event->button() == Qt::RightButton) {
            if (polygonVertices_.size() >= 3) {
                emit polygonGateDrawn(toVariantList(polygonVertices_));
            }
            clearInteractionDraft();
            event->accept();
            return;
        }

        QQuickItem::mousePressEvent(event);
        return;
    }

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

void ScatterPlotItem::mouseMoveEvent(QMouseEvent *event) {
    if (isPolygonMode()) {
        if (!polygonVertices_.isEmpty()) {
            polygonHover_ = mapPlotToData(event->localPos(), dataRect(), plotRect());
            polygonHasHover_ = true;
            update();
            event->accept();
            return;
        }

        QQuickItem::mouseMoveEvent(event);
        return;
    }

    if (!dragging_) {
        QQuickItem::mouseMoveEvent(event);
        return;
    }

    dragCurrent_ = event->localPos();
    update();
    event->accept();
}

void ScatterPlotItem::mouseReleaseEvent(QMouseEvent *event) {
    if (isPolygonMode()) {
        event->accept();
        return;
    }

    if (!dragging_ || event->button() != Qt::LeftButton) {
        QQuickItem::mouseReleaseEvent(event);
        return;
    }

    dragCurrent_ = event->localPos();
    const QRectF plotArea = plotRect();
    const QRectF bounds = dataRect();
    const QRectF activeSelection = selectionRect();
    const bool validSelection = activeSelection.width() >= kMinimumDragPixels
        && activeSelection.height() >= kMinimumDragPixels;

    dragging_ = false;
    update();

    if (validSelection) {
        const QPointF minData = mapPlotToData(activeSelection.topLeft(), bounds, plotArea);
        const QPointF maxData = mapPlotToData(activeSelection.bottomRight(), bounds, plotArea);
        emit rectangleGateDrawn(minData.x(), maxData.x(), maxData.y(), minData.y());
    }

    event->accept();
}

QVector<QPointF> ScatterPlotItem::toPointVector(const QVariantList &values) {
    QVector<QPointF> points;
    points.reserve(values.size());
    for (const QVariant &value : values) {
        const QVariantMap map = value.toMap();
        points.push_back(QPointF(map.value("x").toDouble(), map.value("y").toDouble()));
    }
    return points;
}

QVector<QPointF> ScatterPlotItem::toPointVector(const QVariantMap &columns) {
    const QVariantList xValues = columns.value("x_values").toList();
    const QVariantList yValues = columns.value("y_values").toList();
    const int count = qMin(xValues.size(), yValues.size());

    QVector<QPointF> points;
    points.reserve(count);
    for (int index = 0; index < count; ++index) {
        points.push_back(QPointF(xValues.at(index).toDouble(), yValues.at(index).toDouble()));
    }
    return points;
}

QVariantList ScatterPlotItem::toVariantList(const QVector<QPointF> &values) {
    QVariantList points;
    points.reserve(values.size());
    for (const QPointF &value : values) {
        QVariantMap point;
        point.insert("x", value.x());
        point.insert("y", value.y());
        points.push_back(point);
    }
    return points;
}

QRectF ScatterPlotItem::dataRect() const {
    if (std::isfinite(xMin_) && std::isfinite(xMax_) && std::isfinite(yMin_) && std::isfinite(yMax_)
        && !sameRange(xMin_, xMax_) && !sameRange(yMin_, yMax_)) {
        return QRectF(QPointF(qMin(xMin_, xMax_), qMin(yMin_, yMax_)),
            QPointF(qMax(xMin_, xMax_), qMax(yMin_, yMax_)));
    }

    const QVector<QPointF> *source = !allPointBuffer_.isEmpty() ? &allPointBuffer_ : &highlightPointBuffer_;
    if (source->isEmpty()) {
        return QRectF(0.0, 0.0, 1.0, 1.0);
    }

    qreal minX = source->first().x();
    qreal maxX = source->first().x();
    qreal minY = source->first().y();
    qreal maxY = source->first().y();
    for (const QPointF &point : *source) {
        minX = qMin(minX, point.x());
        maxX = qMax(maxX, point.x());
        minY = qMin(minY, point.y());
        maxY = qMax(maxY, point.y());
    }

    if (qFuzzyCompare(minX, maxX)) {
        minX -= 1.0;
        maxX += 1.0;
    }
    if (qFuzzyCompare(minY, maxY)) {
        minY -= 1.0;
        maxY += 1.0;
    }

    return QRectF(QPointF(minX, minY), QPointF(maxX, maxY));
}

QRectF ScatterPlotItem::plotRect() const {
    return QRectF(
        kPlotPadding,
        kPlotPadding,
        qMax<qreal>(1.0, width() - (kPlotPadding * 2.0)),
        qMax<qreal>(1.0, height() - (kPlotPadding * 2.0)));
}

QRectF ScatterPlotItem::selectionRect() const {
    return QRectF(dragStart_, dragCurrent_).normalized();
}

QPointF ScatterPlotItem::mapDataToPlot(
    const QPointF &point,
    const QRectF &bounds,
    const QRectF &plotArea) const {
    const qreal xNorm = (point.x() - bounds.left()) / bounds.width();
    const qreal yNorm = (point.y() - bounds.top()) / bounds.height();
    const qreal x = plotArea.left() + (xNorm * plotArea.width());
    const qreal y = plotArea.bottom() - (yNorm * plotArea.height());
    return QPointF(x, y);
}

QPointF ScatterPlotItem::mapPlotToData(
    const QPointF &point,
    const QRectF &bounds,
    const QRectF &plotArea) const {
    const qreal clampedX = qBound(plotArea.left(), point.x(), plotArea.right());
    const qreal clampedY = qBound(plotArea.top(), point.y(), plotArea.bottom());
    const qreal xNorm = (clampedX - plotArea.left()) / plotArea.width();
    const qreal yNorm = (plotArea.bottom() - clampedY) / plotArea.height();
    const qreal x = bounds.left() + (xNorm * bounds.width());
    const qreal y = bounds.top() + (yNorm * bounds.height());
    return QPointF(x, y);
}

bool ScatterPlotItem::isPolygonMode() const {
    return interactionMode_ == "polygon";
}

void ScatterPlotItem::clearInteractionDraft() {
    dragging_ = false;
    polygonVertices_.clear();
    polygonHasHover_ = false;
    update();
}

QSGGeometryNode *ScatterPlotItem::buildSeriesNode(
    const QVector<QPointF> &points,
    const QColor &color,
    qreal pointSize,
    const QRectF &bounds,
    const QRectF &plotArea) const {
    auto *geometry = new QSGGeometry(QSGGeometry::defaultAttributes_Point2D(), points.size() * 6);
    geometry->setDrawingMode(QSGGeometry::DrawTriangles);
    auto *vertices = geometry->vertexDataAsPoint2D();

    const qreal half = pointSize * 0.5;

    int offset = 0;
    for (const QPointF &point : points) {
        const QPointF mapped = mapDataToPlot(point, bounds, plotArea);
        const float left = static_cast<float>(mapped.x() - half);
        const float right = static_cast<float>(mapped.x() + half);
        const float top = static_cast<float>(mapped.y() - half);
        const float bottom = static_cast<float>(mapped.y() + half);

        vertices[offset++].set(left, top);
        vertices[offset++].set(right, top);
        vertices[offset++].set(left, bottom);
        vertices[offset++].set(left, bottom);
        vertices[offset++].set(right, top);
        vertices[offset++].set(right, bottom);
    }

    auto *material = new QSGFlatColorMaterial();
    material->setColor(color);

    auto *node = new QSGGeometryNode();
    node->setGeometry(geometry);
    node->setMaterial(material);
    node->setFlag(QSGNode::OwnsGeometry, true);
    node->setFlag(QSGNode::OwnsMaterial, true);
    return node;
}

QSGGeometryNode *ScatterPlotItem::buildSelectionNode(const QRectF &selectionRect) const {
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

    QColor selectionFill("#ef8354");
    selectionFill.setAlphaF(0.22);

    auto *material = new QSGFlatColorMaterial();
    material->setColor(selectionFill);

    auto *node = new QSGGeometryNode();
    node->setGeometry(geometry);
    node->setMaterial(material);
    node->setFlag(QSGNode::OwnsGeometry, true);
    node->setFlag(QSGNode::OwnsMaterial, true);
    return node;
}

QSGGeometryNode *ScatterPlotItem::buildPolylineNode(
    const QVector<QPointF> &points,
    const QColor &color,
    const QRectF &bounds,
    const QRectF &plotArea) const {
    auto *geometry = new QSGGeometry(QSGGeometry::defaultAttributes_Point2D(), points.size());
    geometry->setDrawingMode(QSGGeometry::DrawLineStrip);
    auto *vertices = geometry->vertexDataAsPoint2D();

    int index = 0;
    for (const QPointF &point : points) {
        const QPointF mapped = mapDataToPlot(point, bounds, plotArea);
        vertices[index++].set(static_cast<float>(mapped.x()), static_cast<float>(mapped.y()));
    }

    auto *material = new QSGFlatColorMaterial();
    material->setColor(color);

    auto *node = new QSGGeometryNode();
    node->setGeometry(geometry);
    node->setMaterial(material);
    node->setFlag(QSGNode::OwnsGeometry, true);
    node->setFlag(QSGNode::OwnsMaterial, true);
    return node;
}
