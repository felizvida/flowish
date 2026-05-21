#pragma once

#include <QList>
#include <QRectF>
#include <QSizeF>

QRectF fitFigureIntoSlot(const QSizeF &imageSize, const QRectF &slotRect);
QList<QRectF> stackFigureReportRects(
    const QList<QSizeF> &imageSizes,
    const QRectF &contentRect,
    qreal gap);
