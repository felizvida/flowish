#include "DesktopFigureExport.h"

#include <Qt>
#include <algorithm>

QRectF fitFigureIntoSlot(const QSizeF &imageSize, const QRectF &slotRect) {
    if (imageSize.width() <= 0.0 || imageSize.height() <= 0.0
        || slotRect.width() <= 0.0 || slotRect.height() <= 0.0) {
        return QRectF(slotRect.topLeft(), QSizeF());
    }

    QSizeF scaledSize(imageSize);
    scaledSize.scale(slotRect.size(), Qt::KeepAspectRatio);
    return QRectF(
        slotRect.left() + ((slotRect.width() - scaledSize.width()) / 2.0),
        slotRect.top() + ((slotRect.height() - scaledSize.height()) / 2.0),
        scaledSize.width(),
        scaledSize.height());
}

QList<QRectF> stackFigureReportRects(
    const QList<QSizeF> &imageSizes,
    const QRectF &contentRect,
    qreal gap) {
    QList<QRectF> rects;
    if (imageSizes.isEmpty()) {
        return rects;
    }

    const qreal normalizedGap = std::max<qreal>(0.0, gap);
    const qreal slotHeight = std::max<qreal>(
        1.0,
        (contentRect.height() - (normalizedGap * (imageSizes.size() - 1)))
            / imageSizes.size());
    qreal y = contentRect.top();
    for (const QSizeF &imageSize : imageSizes) {
        const QRectF slotRect(contentRect.left(), y, contentRect.width(), slotHeight);
        rects.append(fitFigureIntoSlot(imageSize, slotRect));
        y += slotHeight + normalizedGap;
    }
    return rects;
}
