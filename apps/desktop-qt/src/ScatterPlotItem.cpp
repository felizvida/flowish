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

QVariantMap ScatterPlotItem::densityGrid() const {
    return densityGrid_;
}

QVariantList ScatterPlotItem::gateOverlays() const {
    return gateOverlays_;
}

QString ScatterPlotItem::selectedPopulationKey() const {
    return selectedPopulationKey_;
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

void ScatterPlotItem::setDensityGrid(const QVariantMap &densityGrid) {
    if (densityGrid == densityGrid_) {
        return;
    }

    densityGrid_ = densityGrid;
    densityCellBuffer_ = toDensityCells(densityGrid);
    update();
    emit densityGridChanged();
}

void ScatterPlotItem::setGateOverlays(const QVariantList &overlays) {
    if (overlays == gateOverlays_) {
        return;
    }

    gateOverlays_ = overlays;
    gateOverlayBuffer_ = toGateOverlays(overlays);
    update();
    emit gateOverlaysChanged();
}

void ScatterPlotItem::setSelectedPopulationKey(const QString &populationKey) {
    const QString nextKey = populationKey.trimmed().isEmpty() ? QStringLiteral("__all__") : populationKey;
    if (selectedPopulationKey_ == nextKey) {
        return;
    }

    selectedPopulationKey_ = nextKey;
    update();
    emit selectedPopulationKeyChanged();
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
    QString nextMode = QStringLiteral("rectangle");
    if (normalized == "polygon") {
        nextMode = QStringLiteral("polygon");
    } else if (normalized == "pan") {
        nextMode = QStringLiteral("pan");
    }
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
    constexpr int densityBandCount = 5;
    for (int band = 0; band < densityBandCount; ++band) {
        if (QSGGeometryNode *densityNode =
                buildDensityNode(densityCellBuffer_, band, densityBandCount, bounds, plotArea)) {
            root->appendChildNode(densityNode);
        }
    }

    QColor allPointColor("#365b63");
    if (!densityCellBuffer_.isEmpty()) {
        allPointColor.setAlphaF(0.64);
    }
    root->appendChildNode(buildSeriesNode(allPointBuffer_, allPointColor, 6.0, bounds, plotArea));
    if (!highlightPointBuffer_.isEmpty()) {
        root->appendChildNode(buildSeriesNode(
            highlightPointBuffer_,
            QColor("#ef8354"),
            10.0,
            bounds,
            plotArea));
    }
    for (const GateOverlay &overlay : gateOverlayBuffer_) {
        if (overlay.populationId != selectedPopulationKey_) {
            root->appendChildNode(buildPolylineNode(overlay.vertices, QColor("#6f8b8f"), bounds, plotArea));
        }
    }
    for (const GateOverlay &overlay : gateOverlayBuffer_) {
        if (overlay.populationId == selectedPopulationKey_) {
            root->appendChildNode(buildPolylineNode(overlay.vertices, QColor("#ef8354"), bounds, plotArea));
        }
    }
    if (dragging_ && !isPanMode()) {
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
    if (isPanMode()) {
        const bool validPan =
            qAbs(dragCurrent_.x() - dragStart_.x()) >= kMinimumDragPixels
            || qAbs(dragCurrent_.y() - dragStart_.y()) >= kMinimumDragPixels;
        dragging_ = false;
        update();

        if (validPan) {
            const QPointF startData = mapPlotToData(dragStart_, bounds, plotArea);
            const QPointF endData = mapPlotToData(dragCurrent_, bounds, plotArea);
            emit plotPanned(startData.x() - endData.x(), startData.y() - endData.y());
        }

        event->accept();
        return;
    }

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

QVector<ScatterPlotItem::DensityCell> ScatterPlotItem::toDensityCells(const QVariantMap &densityGrid) {
    const QVariantList cellValues = densityGrid.value("cells").toList();
    QVector<DensityCell> cells;
    cells.reserve(cellValues.size());
    for (const QVariant &cellValue : cellValues) {
        const QVariantMap cellMap = cellValue.toMap();
        bool okXMin = false;
        bool okXMax = false;
        bool okYMin = false;
        bool okYMax = false;
        bool okIntensity = false;
        const double xMin = cellMap.value("x_min").toDouble(&okXMin);
        const double xMax = cellMap.value("x_max").toDouble(&okXMax);
        const double yMin = cellMap.value("y_min").toDouble(&okYMin);
        const double yMax = cellMap.value("y_max").toDouble(&okYMax);
        const double intensity = cellMap.value("intensity").toDouble(&okIntensity);
        if (!okXMin || !okXMax || !okYMin || !okYMax || !okIntensity) {
            continue;
        }
        if (!std::isfinite(xMin) || !std::isfinite(xMax) || !std::isfinite(yMin) || !std::isfinite(yMax)
            || !std::isfinite(intensity) || xMax <= xMin || yMax <= yMin || intensity <= 0.0) {
            continue;
        }

        cells.push_back(DensityCell {
            QRectF(QPointF(xMin, yMin), QPointF(xMax, yMax)).normalized(),
            qBound(0.0, intensity, 1.0),
        });
    }
    return cells;
}

QVector<ScatterPlotItem::GateOverlay> ScatterPlotItem::toGateOverlays(const QVariantList &values) {
    QVector<GateOverlay> overlays;
    overlays.reserve(values.size());
    for (const QVariant &value : values) {
        const QVariantMap overlayMap = value.toMap();
        const QVariantList vertexValues = overlayMap.value("vertices").toList();
        QVector<QPointF> vertices;
        vertices.reserve(vertexValues.size());
        for (const QVariant &vertexValue : vertexValues) {
            const QVariantMap vertexMap = vertexValue.toMap();
            bool okX = false;
            bool okY = false;
            const double x = vertexMap.value("x").toDouble(&okX);
            const double y = vertexMap.value("y").toDouble(&okY);
            if (okX && okY && std::isfinite(x) && std::isfinite(y)) {
                vertices.push_back(QPointF(x, y));
            }
        }

        if (vertices.size() >= 2) {
            overlays.push_back(GateOverlay {
                overlayMap.value("population_id").toString(),
                vertices,
            });
        }
    }
    return overlays;
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

bool ScatterPlotItem::isPanMode() const {
    return interactionMode_ == "pan";
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

QSGGeometryNode *ScatterPlotItem::buildDensityNode(
    const QVector<DensityCell> &cells,
    int band,
    int bandCount,
    const QRectF &bounds,
    const QRectF &plotArea) const {
    if (cells.isEmpty() || bandCount <= 0 || band < 0 || band >= bandCount) {
        return nullptr;
    }

    const double lower = static_cast<double>(band) / static_cast<double>(bandCount);
    const double upper = static_cast<double>(band + 1) / static_cast<double>(bandCount);
    int cellCount = 0;
    for (const DensityCell &cell : cells) {
        if (cell.intensity > lower && cell.intensity <= upper) {
            ++cellCount;
        }
    }
    if (cellCount == 0) {
        return nullptr;
    }

    auto *geometry = new QSGGeometry(QSGGeometry::defaultAttributes_Point2D(), cellCount * 6);
    geometry->setDrawingMode(QSGGeometry::DrawTriangles);
    auto *vertices = geometry->vertexDataAsPoint2D();

    int offset = 0;
    for (const DensityCell &cell : cells) {
        if (cell.intensity <= lower || cell.intensity > upper) {
            continue;
        }

        const QPointF low = mapDataToPlot(QPointF(cell.bounds.left(), cell.bounds.top()), bounds, plotArea);
        const QPointF high = mapDataToPlot(QPointF(cell.bounds.right(), cell.bounds.bottom()), bounds, plotArea);
        const float left = static_cast<float>(qMin(low.x(), high.x()));
        const float right = static_cast<float>(qMax(low.x(), high.x()));
        const float top = static_cast<float>(qMin(low.y(), high.y()));
        const float bottom = static_cast<float>(qMax(low.y(), high.y()));

        vertices[offset++].set(left, top);
        vertices[offset++].set(right, top);
        vertices[offset++].set(left, bottom);
        vertices[offset++].set(left, bottom);
        vertices[offset++].set(right, top);
        vertices[offset++].set(right, bottom);
    }

    QColor color("#d8913c");
    color.setAlphaF(0.10 + (0.07 * static_cast<double>(band)));

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
