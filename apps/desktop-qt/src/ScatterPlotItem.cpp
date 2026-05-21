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
constexpr qreal kEditHandleHitPixels = 10.0;

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
    } else if (normalized == "edit") {
        nextMode = QStringLiteral("edit");
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
    if (const GateOverlay *editableOverlay = selectedRectangleOverlay()) {
        if (isEditMode() && !rectangleEdit_.active) {
            QVector<QPointF> handles = rectangleVertices(overlayBoundsData(*editableOverlay));
            if (!handles.isEmpty()) {
                handles.removeLast();
            }
            root->appendChildNode(buildSeriesNode(handles, QColor("#f4a259"), 13.0, bounds, plotArea));
        }
    }
    if (rectangleEdit_.active) {
        const QRectF editedBounds = editedRectangleBounds(dragCurrent_);
        root->appendChildNode(buildSelectionNode(mapDataBoundsToPlot(editedBounds, bounds, plotArea)));
        root->appendChildNode(buildPolylineNode(rectangleVertices(editedBounds), QColor("#f4a259"), bounds, plotArea));
        QVector<QPointF> handles = rectangleVertices(editedBounds);
        if (!handles.isEmpty()) {
            handles.removeLast();
        }
        root->appendChildNode(buildSeriesNode(handles, QColor("#f4a259"), 13.0, bounds, plotArea));
    }
    if (const GateOverlay *editableOverlay = selectedPolygonOverlay()) {
        if (isEditMode() && !polygonEdit_.active) {
            root->appendChildNode(buildSeriesNode(
                editablePolygonVertices(*editableOverlay),
                QColor("#f4a259"),
                13.0,
                bounds,
                plotArea));
        }
    }
    if (polygonEdit_.active) {
        const QVector<QPointF> editedVertices = editedPolygonVertices(dragCurrent_);
        root->appendChildNode(buildPolylineNode(closedPolygonPath(editedVertices), QColor("#f4a259"), bounds, plotArea));
        root->appendChildNode(buildSeriesNode(editedVertices, QColor("#f4a259"), 13.0, bounds, plotArea));
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

    if (isEditMode()) {
        if (beginRectangleEdit(event->localPos()) || beginPolygonEdit(event->localPos())) {
            event->accept();
            return;
        }

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

    if (rectangleEdit_.active) {
        dragCurrent_ = event->localPos();
        update();
        event->accept();
        return;
    }

    if (polygonEdit_.active) {
        dragCurrent_ = event->localPos();
        update();
        event->accept();
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

    if (rectangleEdit_.active && event->button() == Qt::LeftButton) {
        dragCurrent_ = event->localPos();
        const QRectF editedBounds = editedRectangleBounds(dragCurrent_);
        const QString populationId = rectangleEdit_.populationId;
        const QRectF originalBounds = rectangleEdit_.startBounds;
        rectangleEdit_ = RectangleEditState {};
        update();

        const bool changed = qAbs(editedBounds.left() - originalBounds.left()) > 1e-9
            || qAbs(editedBounds.right() - originalBounds.right()) > 1e-9
            || qAbs(editedBounds.top() - originalBounds.top()) > 1e-9
            || qAbs(editedBounds.bottom() - originalBounds.bottom()) > 1e-9;
        if (changed && editedBounds.width() > 0.0 && editedBounds.height() > 0.0) {
            emit rectangleGateEdited(
                populationId,
                editedBounds.left(),
                editedBounds.right(),
                editedBounds.top(),
                editedBounds.bottom());
        }

        event->accept();
        return;
    }

    if (polygonEdit_.active && event->button() == Qt::LeftButton) {
        dragCurrent_ = event->localPos();
        const QVector<QPointF> editedVertices = editedPolygonVertices(dragCurrent_);
        const QString populationId = polygonEdit_.populationId;
        const QVector<QPointF> originalVertices = polygonEdit_.startVertices;
        polygonEdit_ = PolygonEditState {};
        update();

        if (editedVertices.size() >= 3 && polygonVerticesChanged(editedVertices, originalVertices)) {
            emit polygonGateEdited(populationId, toVariantList(editedVertices));
        }

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
            QString kind = overlayMap.value("kind").toString().trimmed().toLower();
            if (kind.isEmpty()) {
                kind = vertices.size() == 5 ? QStringLiteral("rectangle") : QStringLiteral("polygon");
            }
            overlays.push_back(GateOverlay {
                kind,
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

bool ScatterPlotItem::isEditMode() const {
    return interactionMode_ == "edit";
}

void ScatterPlotItem::clearInteractionDraft() {
    dragging_ = false;
    rectangleEdit_ = RectangleEditState {};
    polygonEdit_ = PolygonEditState {};
    polygonVertices_.clear();
    polygonHasHover_ = false;
    update();
}

const ScatterPlotItem::GateOverlay *ScatterPlotItem::selectedRectangleOverlay() const {
    if (selectedPopulationKey_.isEmpty() || selectedPopulationKey_ == "__all__") {
        return nullptr;
    }

    for (const GateOverlay &overlay : gateOverlayBuffer_) {
        if (overlay.populationId == selectedPopulationKey_ && overlay.kind == "rectangle"
            && overlay.vertices.size() >= 4) {
            return &overlay;
        }
    }
    return nullptr;
}

const ScatterPlotItem::GateOverlay *ScatterPlotItem::selectedPolygonOverlay() const {
    if (selectedPopulationKey_.isEmpty() || selectedPopulationKey_ == "__all__") {
        return nullptr;
    }

    for (const GateOverlay &overlay : gateOverlayBuffer_) {
        if (overlay.populationId == selectedPopulationKey_ && overlay.kind == "polygon"
            && editablePolygonVertices(overlay).size() >= 3) {
            return &overlay;
        }
    }
    return nullptr;
}

QRectF ScatterPlotItem::overlayBoundsData(const GateOverlay &overlay) const {
    if (overlay.vertices.isEmpty()) {
        return QRectF();
    }

    qreal left = overlay.vertices.first().x();
    qreal right = left;
    qreal bottom = overlay.vertices.first().y();
    qreal top = bottom;
    for (const QPointF &vertex : overlay.vertices) {
        left = qMin(left, vertex.x());
        right = qMax(right, vertex.x());
        bottom = qMin(bottom, vertex.y());
        top = qMax(top, vertex.y());
    }
    return QRectF(QPointF(left, bottom), QPointF(right, top)).normalized();
}

QRectF ScatterPlotItem::mapDataBoundsToPlot(
    const QRectF &dataBounds,
    const QRectF &bounds,
    const QRectF &plotArea) const {
    const QPointF lowerLeft = mapDataToPlot(QPointF(dataBounds.left(), dataBounds.top()), bounds, plotArea);
    const QPointF upperRight = mapDataToPlot(QPointF(dataBounds.right(), dataBounds.bottom()), bounds, plotArea);
    return QRectF(lowerLeft, upperRight).normalized();
}

ScatterPlotItem::RectangleEditHandle ScatterPlotItem::hitTestRectangleEdit(
    const GateOverlay &overlay,
    const QPointF &plotPosition,
    const QRectF &bounds,
    const QRectF &plotArea) const {
    const QRectF plotBounds = mapDataBoundsToPlot(overlayBoundsData(overlay), bounds, plotArea);
    const QRectF expanded = plotBounds.adjusted(
        -kEditHandleHitPixels,
        -kEditHandleHitPixels,
        kEditHandleHitPixels,
        kEditHandleHitPixels);
    if (!expanded.contains(plotPosition)) {
        return RectangleEditHandle::None;
    }

    const bool nearLeft = qAbs(plotPosition.x() - plotBounds.left()) <= kEditHandleHitPixels;
    const bool nearRight = qAbs(plotPosition.x() - plotBounds.right()) <= kEditHandleHitPixels;
    const bool nearTop = qAbs(plotPosition.y() - plotBounds.top()) <= kEditHandleHitPixels;
    const bool nearBottom = qAbs(plotPosition.y() - plotBounds.bottom()) <= kEditHandleHitPixels;

    if (nearLeft && nearTop) {
        return RectangleEditHandle::TopLeft;
    }
    if (nearRight && nearTop) {
        return RectangleEditHandle::TopRight;
    }
    if (nearLeft && nearBottom) {
        return RectangleEditHandle::BottomLeft;
    }
    if (nearRight && nearBottom) {
        return RectangleEditHandle::BottomRight;
    }
    if (nearLeft) {
        return RectangleEditHandle::Left;
    }
    if (nearRight) {
        return RectangleEditHandle::Right;
    }
    if (nearTop) {
        return RectangleEditHandle::Top;
    }
    if (nearBottom) {
        return RectangleEditHandle::Bottom;
    }
    return plotBounds.contains(plotPosition) ? RectangleEditHandle::Move : RectangleEditHandle::None;
}

bool ScatterPlotItem::beginRectangleEdit(const QPointF &plotPosition) {
    const GateOverlay *overlay = selectedRectangleOverlay();
    if (overlay == nullptr) {
        return false;
    }

    const QRectF bounds = dataRect();
    const QRectF plotArea = plotRect();
    const RectangleEditHandle handle = hitTestRectangleEdit(*overlay, plotPosition, bounds, plotArea);
    if (handle == RectangleEditHandle::None) {
        return false;
    }

    rectangleEdit_.active = true;
    rectangleEdit_.populationId = overlay->populationId;
    rectangleEdit_.startBounds = overlayBoundsData(*overlay);
    rectangleEdit_.startData = mapPlotToData(plotPosition, bounds, plotArea);
    rectangleEdit_.handle = handle;
    dragStart_ = plotPosition;
    dragCurrent_ = plotPosition;
    update();
    return true;
}

QRectF ScatterPlotItem::editedRectangleBounds(const QPointF &plotPosition) const {
    if (!rectangleEdit_.active) {
        return QRectF();
    }

    const QPointF currentData = mapPlotToData(plotPosition, dataRect(), plotRect());
    const qreal xDelta = currentData.x() - rectangleEdit_.startData.x();
    const qreal yDelta = currentData.y() - rectangleEdit_.startData.y();
    qreal left = rectangleEdit_.startBounds.left();
    qreal right = rectangleEdit_.startBounds.right();
    qreal bottom = rectangleEdit_.startBounds.top();
    qreal top = rectangleEdit_.startBounds.bottom();

    switch (rectangleEdit_.handle) {
    case RectangleEditHandle::Move:
        left += xDelta;
        right += xDelta;
        bottom += yDelta;
        top += yDelta;
        break;
    case RectangleEditHandle::Left:
        left += xDelta;
        break;
    case RectangleEditHandle::Right:
        right += xDelta;
        break;
    case RectangleEditHandle::Top:
        top += yDelta;
        break;
    case RectangleEditHandle::Bottom:
        bottom += yDelta;
        break;
    case RectangleEditHandle::TopLeft:
        left += xDelta;
        top += yDelta;
        break;
    case RectangleEditHandle::TopRight:
        right += xDelta;
        top += yDelta;
        break;
    case RectangleEditHandle::BottomLeft:
        left += xDelta;
        bottom += yDelta;
        break;
    case RectangleEditHandle::BottomRight:
        right += xDelta;
        bottom += yDelta;
        break;
    case RectangleEditHandle::None:
        break;
    }

    return QRectF(
        QPointF(qMin(left, right), qMin(bottom, top)),
        QPointF(qMax(left, right), qMax(bottom, top)));
}

QVector<QPointF> ScatterPlotItem::rectangleVertices(const QRectF &dataBounds) const {
    return QVector<QPointF> {
        QPointF(dataBounds.left(), dataBounds.top()),
        QPointF(dataBounds.right(), dataBounds.top()),
        QPointF(dataBounds.right(), dataBounds.bottom()),
        QPointF(dataBounds.left(), dataBounds.bottom()),
        QPointF(dataBounds.left(), dataBounds.top()),
    };
}

QVector<QPointF> ScatterPlotItem::editablePolygonVertices(const GateOverlay &overlay) const {
    QVector<QPointF> vertices = overlay.vertices;
    if (vertices.size() >= 2) {
        const QPointF first = vertices.first();
        const QPointF last = vertices.last();
        if (qAbs(first.x() - last.x()) < 1e-9 && qAbs(first.y() - last.y()) < 1e-9) {
            vertices.removeLast();
        }
    }
    return vertices;
}

QVector<QPointF> ScatterPlotItem::closedPolygonPath(const QVector<QPointF> &vertices) const {
    QVector<QPointF> path = vertices;
    if (!path.isEmpty()) {
        path.push_back(path.first());
    }
    return path;
}

ScatterPlotItem::PolygonEditHit ScatterPlotItem::hitTestPolygonEdit(
    const GateOverlay &overlay,
    const QPointF &plotPosition,
    const QRectF &bounds,
    const QRectF &plotArea) const {
    const QVector<QPointF> vertices = editablePolygonVertices(overlay);
    int nearestVertex = -1;
    qreal nearestDistance = kEditHandleHitPixels + 1.0;
    for (int index = 0; index < vertices.size(); ++index) {
        const QPointF mapped = mapDataToPlot(vertices.at(index), bounds, plotArea);
        const qreal distance = qSqrt(
            qPow(mapped.x() - plotPosition.x(), 2.0) + qPow(mapped.y() - plotPosition.y(), 2.0));
        if (distance <= kEditHandleHitPixels && distance < nearestDistance) {
            nearestDistance = distance;
            nearestVertex = index;
        }
    }

    if (nearestVertex >= 0) {
        return PolygonEditHit {PolygonEditHandle::Vertex, nearestVertex};
    }
    if (polygonContainsPlotPoint(vertices, plotPosition, bounds, plotArea)) {
        return PolygonEditHit {PolygonEditHandle::Move, -1};
    }
    return PolygonEditHit {};
}

bool ScatterPlotItem::polygonContainsPlotPoint(
    const QVector<QPointF> &vertices,
    const QPointF &plotPosition,
    const QRectF &bounds,
    const QRectF &plotArea) const {
    if (vertices.size() < 3) {
        return false;
    }

    bool inside = false;
    int previous = vertices.size() - 1;
    for (int current = 0; current < vertices.size(); ++current) {
        const QPointF currentPoint = mapDataToPlot(vertices.at(current), bounds, plotArea);
        const QPointF previousPoint = mapDataToPlot(vertices.at(previous), bounds, plotArea);
        const bool crossesY =
            (currentPoint.y() > plotPosition.y()) != (previousPoint.y() > plotPosition.y());
        if (crossesY) {
            const qreal intersectionX = currentPoint.x()
                + ((plotPosition.y() - currentPoint.y())
                   * (previousPoint.x() - currentPoint.x())
                   / (previousPoint.y() - currentPoint.y()));
            if (plotPosition.x() < intersectionX) {
                inside = !inside;
            }
        }
        previous = current;
    }
    return inside;
}

bool ScatterPlotItem::beginPolygonEdit(const QPointF &plotPosition) {
    const GateOverlay *overlay = selectedPolygonOverlay();
    if (overlay == nullptr) {
        return false;
    }

    const QRectF bounds = dataRect();
    const QRectF plotArea = plotRect();
    const PolygonEditHit hit = hitTestPolygonEdit(*overlay, plotPosition, bounds, plotArea);
    if (hit.handle == PolygonEditHandle::None) {
        return false;
    }

    polygonEdit_.active = true;
    polygonEdit_.populationId = overlay->populationId;
    polygonEdit_.startVertices = editablePolygonVertices(*overlay);
    polygonEdit_.startData = mapPlotToData(plotPosition, bounds, plotArea);
    polygonEdit_.handle = hit.handle;
    polygonEdit_.vertexIndex = hit.vertexIndex;
    dragStart_ = plotPosition;
    dragCurrent_ = plotPosition;
    update();
    return true;
}

QVector<QPointF> ScatterPlotItem::editedPolygonVertices(const QPointF &plotPosition) const {
    QVector<QPointF> vertices = polygonEdit_.startVertices;
    if (!polygonEdit_.active || vertices.isEmpty()) {
        return vertices;
    }

    const QPointF currentData = mapPlotToData(plotPosition, dataRect(), plotRect());
    const QPointF delta(
        currentData.x() - polygonEdit_.startData.x(),
        currentData.y() - polygonEdit_.startData.y());

    if (polygonEdit_.handle == PolygonEditHandle::Move) {
        for (QPointF &vertex : vertices) {
            vertex += delta;
        }
    } else if (polygonEdit_.handle == PolygonEditHandle::Vertex
               && polygonEdit_.vertexIndex >= 0
               && polygonEdit_.vertexIndex < vertices.size()) {
        vertices[polygonEdit_.vertexIndex] = currentData;
    }
    return vertices;
}

bool ScatterPlotItem::polygonVerticesChanged(
    const QVector<QPointF> &left,
    const QVector<QPointF> &right) const {
    if (left.size() != right.size()) {
        return true;
    }
    for (int index = 0; index < left.size(); ++index) {
        if (qAbs(left.at(index).x() - right.at(index).x()) > 1e-9
            || qAbs(left.at(index).y() - right.at(index).y()) > 1e-9) {
            return true;
        }
    }
    return false;
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
