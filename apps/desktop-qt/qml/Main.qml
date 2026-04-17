import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import Flowjoish 1.0

ApplicationWindow {
    id: window
    width: 1460
    height: 920
    visible: true
    title: "Parallax"
    color: "#efe7d7"

    property var plotA: desktopController.plots.length > 0 ? desktopController.plots[0] : ({})
    property var plotB: desktopController.plots.length > 1 ? desktopController.plots[1] : ({})
    property var plotC: desktopController.plots.length > 2 ? desktopController.plots[2] : ({})
    property string activeGateTool: "rectangle"
    property string derivedMetricDraftKind: "positive_fraction"
    property string derivedMetricDraftChannel: ""
    property string derivedMetricDraftNumeratorChannel: ""
    property string derivedMetricDraftDenominatorChannel: ""
    property string derivedMetricDraftThreshold: "1.00"

    function transformIndex(kind) {
        if (kind === "signed_log10")
            return 1
        if (kind === "asinh")
            return 2
        if (kind === "biexponential")
            return 3
        if (kind === "logicle")
            return 4
        return 0
    }

    function transformKindAt(index) {
        if (index === 1)
            return "signed_log10"
        if (index === 2)
            return "asinh"
        if (index === 3)
            return "biexponential"
        if (index === 4)
            return "logicle"
        return "linear"
    }

    function plotAxisLabel(plot) {
        if ((plot.kind || "") === "histogram")
            return (plot.x_channel || "Channel") + " histogram"
        return (plot.x_channel || "x") + " vs " + (plot.y_channel || "y")
    }

    function plotHelperText(plot) {
        if ((plot.kind || "") === "histogram")
            return "Histogram view is read-only today; use Auto, Focus, and Zoom to inspect distributions."
        return window.activeGateTool === "rectangle"
                ? "Drag to author a rectangle gate on this projection"
                : "Click to place polygon vertices, then right-click to finish"
    }

    function formatPercent(value) {
        const numeric = Number(value)
        if (!isFinite(numeric))
            return "0.0%"
        return (numeric * 100).toFixed(1) + "%"
    }

    function formatSignedPercent(value) {
        const numeric = Number(value)
        if (!isFinite(numeric))
            return "n/a"
        const scaled = numeric * 100
        const prefix = scaled > 0.0001 ? "+" : ""
        return prefix + scaled.toFixed(1) + "%"
    }

    function formatPercentOrNA(value) {
        const numeric = Number(value)
        if (!isFinite(numeric))
            return "n/a"
        return (numeric * 100).toFixed(1) + "%"
    }

    function formatStatValue(value) {
        const numeric = Number(value)
        if (!isFinite(numeric))
            return "n/a"
        return numeric.toFixed(2)
    }

    function listIndex(values, value) {
        for (let index = 0; index < values.length; ++index) {
            if (values[index] === value)
                return index
        }
        return values.length > 0 ? 0 : -1
    }

    function metricChannels() {
        return desktopController.sample.channels || []
    }

    function fallbackChannel(channels, preferredIndex) {
        if (channels.length === 0)
            return ""
        const index = Math.min(Math.max(preferredIndex, 0), channels.length - 1)
        return channels[index]
    }

    function syncDerivedMetricDraft() {
        const metric = desktopController.derivedMetric || {}
        const channels = window.metricChannels()
        window.derivedMetricDraftKind = metric.kind || "positive_fraction"
        window.derivedMetricDraftChannel = metric.channel || window.fallbackChannel(channels, 0)
        window.derivedMetricDraftNumeratorChannel =
                metric.numerator_channel || window.fallbackChannel(channels, 0)
        window.derivedMetricDraftDenominatorChannel =
                metric.denominator_channel || window.fallbackChannel(channels, channels.length > 1 ? 1 : 0)
        const threshold = Number(metric.threshold)
        window.derivedMetricDraftThreshold = isFinite(threshold) ? threshold.toFixed(2) : "1.00"
    }

    function formatDerivedMetricValue(value, kind) {
        const numeric = Number(value)
        if (!isFinite(numeric))
            return "n/a"
        if (kind === "positive_fraction")
            return window.formatPercent(numeric)
        return numeric.toFixed(3)
    }

    function formatSignedDerivedMetricValue(value, kind) {
        const numeric = Number(value)
        if (!isFinite(numeric))
            return "n/a"
        if (kind === "positive_fraction")
            return window.formatSignedPercent(numeric)
        const prefix = numeric > 0.0001 ? "+" : ""
        return prefix + numeric.toFixed(3)
    }

    function derivedMetricLabel() {
        return desktopController.derivedMetric.label || "Derived metric"
    }

    Component.onCompleted: window.syncDerivedMetricDraft()

    Connections {
        target: desktopController

        function onSnapshotChanged() {
            window.syncDerivedMetricDraft()
        }
    }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: "#faf7f0" }
            GradientStop { position: 1.0; color: "#e4d6bc" }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 24
        spacing: 18

        Rectangle {
            Layout.fillWidth: true
            radius: 24
            color: "#17342a"
            border.width: 1
            border.color: "#345246"
            implicitHeight: 168

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 24
                spacing: 10

                Text {
                    text: "Parallax"
                    color: "#f3efe5"
                    font.pixelSize: 34
                    font.weight: Font.DemiBold
                }

                Text {
                    text: "A local-first cytometry workstation backed by a stateful Rust analysis session"
                    color: "#c9ddd1"
                    font.pixelSize: 18
                }

                RowLayout {
                    spacing: 14

                    Rectangle {
                        radius: 999
                        color: "#29463b"
                        implicitHeight: 34
                        implicitWidth: statusLabel.width + 26

                        Text {
                            id: statusLabel
                            anchors.centerIn: parent
                            text: "Status: " + desktopController.status
                            color: "#f7f3ea"
                            font.pixelSize: 14
                            font.weight: Font.Medium
                        }
                    }

                    Text {
                        text: "Sample " + (desktopController.sample.display_name || "Demo Sample")
                        color: "#d6e6dc"
                        font.pixelSize: 14
                    }

                    Text {
                        text: "Events " + (desktopController.sample.event_count || 0)
                        color: "#d6e6dc"
                        font.pixelSize: 14
                    }

                    Text {
                        text: "Commands " + desktopController.commandCount
                        color: "#d6e6dc"
                        font.pixelSize: 14
                    }

                    Text {
                        text: "Log " + desktopController.commandLogHash
                        color: "#9cb7a9"
                        font.family: "Menlo"
                        font.pixelSize: 13
                    }

                    Text {
                        text: "Exec " + desktopController.executionHash
                        color: "#9cb7a9"
                        font.family: "Menlo"
                        font.pixelSize: 13
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 18

            Rectangle {
                Layout.preferredWidth: 320
                Layout.fillHeight: true
                radius: 22
                color: "#fffaf1"
                border.width: 1
                border.color: "#dcc8a0"

                ScrollView {
                    anchors.fill: parent
                    anchors.margins: 20

                    Column {
                        width: parent.width
                        spacing: 18

                        Column {
                            width: parent.width
                            spacing: 10

                            Text {
                                text: "Samples"
                                color: "#2e2216"
                                font.pixelSize: 22
                                font.weight: Font.DemiBold
                            }

                            Button {
                                text: "Import FCS Files"
                                onClicked: desktopController.importFcsFiles()
                            }

                            Row {
                                spacing: 10

                                Button {
                                    text: "Load Workspace"
                                    onClicked: desktopController.loadWorkspace()
                                }

                                Button {
                                    text: "Save Workspace As"
                                    onClicked: desktopController.saveWorkspaceAs()
                                }
                            }

                            Button {
                                text: "Export Stats CSV"
                                onClicked: desktopController.exportStatsCsv()
                            }

                            Button {
                                text: "Apply Template To Other Samples"
                                enabled: desktopController.samples.length > 1
                                onClicked: desktopController.applyActiveTemplateToOtherSamples()
                            }

                            Button {
                                text: "Export Batch Stats CSV"
                                enabled: desktopController.samples.length > 1
                                onClicked: desktopController.exportBatchStatsCsv()
                            }

                            Button {
                                text: "Export Selected Comparison CSV"
                                enabled: desktopController.samples.length > 1
                                onClicked: desktopController.exportSelectedPopulationComparisonCsv()
                            }

                            Button {
                                text: "Export Cohort Summary CSV"
                                enabled: desktopController.samples.length > 1
                                onClicked: desktopController.exportSelectedPopulationGroupSummaryCsv()
                            }

                            Button {
                                text: "Export Derived Metric CSV"
                                onClicked: desktopController.exportSelectedPopulationDerivedMetricCsv()
                            }

                            Text {
                                width: parent.width
                                text: desktopController.samples.length > 1
                                      ? "Switch between imported samples without leaving the local Rust session."
                                      : "Import one or more FCS files to replace the demo sample with a multi-sample session."
                                color: "#6d5941"
                                font.pixelSize: 13
                                wrapMode: Text.WordWrap
                            }

                            Text {
                                width: parent.width
                                text: desktopController.samples.length > 1
                                      ? "Batch actions use the active sample's current gate log as a template for the other loaded samples. Applying the template replaces gate history on the other samples, but keeps each sample's own analysis settings."
                                      : "Batch workflows appear after you load more than one sample."
                                color: "#6d5941"
                                font.pixelSize: 13
                                wrapMode: Text.WordWrap
                            }

                            Text {
                                width: parent.width
                                text: desktopController.workspacePath === ""
                                      ? "Workspace: not saved yet"
                                      : "Workspace: " + desktopController.workspacePath
                                color: "#8a7354"
                                font.pixelSize: 12
                                wrapMode: Text.WordWrap
                            }

                            Text {
                                text: "Active Sample Group"
                                color: "#2e2216"
                                font.pixelSize: 15
                                font.weight: Font.Medium
                            }

                            Row {
                                width: parent.width
                                spacing: 8

                                TextField {
                                    id: activeSampleGroupField
                                    width: parent.width - 128
                                    text: desktopController.sample.group_label || "Ungrouped"
                                    placeholderText: "Ungrouped"
                                    selectByMouse: true
                                    onEditingFinished: desktopController.setActiveSampleGroupLabel(text)
                                }

                                Button {
                                    text: "Apply"
                                    onClicked: desktopController.setActiveSampleGroupLabel(activeSampleGroupField.text)
                                }
                            }

                            Repeater {
                                model: desktopController.samples

                                delegate: Rectangle {
                                    width: parent.width
                                    radius: 14
                                    color: modelData.id === desktopController.selectedSampleId
                                           ? "#dfe8e2"
                                           : "#f6efe1"
                                    border.width: 1
                                    border.color: modelData.id === desktopController.selectedSampleId
                                                  ? "#6f8a7b"
                                                  : "#dcc8a0"
                                    implicitHeight: 92

                                    MouseArea {
                                        anchors.fill: parent
                                        onClicked: desktopController.selectedSampleId = modelData.id
                                    }

                                    Column {
                                        anchors.fill: parent
                                        anchors.margins: 12
                                        spacing: 4

                                        Text {
                                            text: modelData.display_name
                                            color: "#2e2216"
                                            font.pixelSize: 16
                                            font.weight: Font.DemiBold
                                        }

                                        Text {
                                            text: (modelData.event_count || 0) + " events • "
                                                  + (modelData.channel_count || 0) + " channels"
                                            color: "#6d5941"
                                            font.pixelSize: 13
                                        }

                                        Text {
                                            text: "Group: " + (modelData.group_label || "Ungrouped")
                                            color: "#6d5941"
                                            font.pixelSize: 13
                                        }

                                        Text {
                                            width: parent.width
                                            text: modelData.source_path || modelData.id
                                            color: "#8a7354"
                                            font.pixelSize: 12
                                            elide: Text.ElideLeft
                                        }
                                    }
                                }
                            }
                        }

                        Column {
                            width: parent.width
                            spacing: 10

                            Text {
                                text: "Derived Metric"
                                color: "#2e2216"
                                font.pixelSize: 22
                                font.weight: Font.DemiBold
                            }

                            Text {
                                width: parent.width
                                text: window.derivedMetricLabel()
                                      + " is evaluated on the selected population for every loaded sample."
                                color: "#6d5941"
                                font.pixelSize: 13
                                wrapMode: Text.WordWrap
                            }

                            ComboBox {
                                width: parent.width
                                model: [
                                    "Positive Fraction",
                                    "Mean Ratio"
                                ]
                                currentIndex: window.derivedMetricDraftKind === "mean_ratio" ? 1 : 0
                                onActivated: {
                                    window.derivedMetricDraftKind = currentIndex === 1
                                            ? "mean_ratio"
                                            : "positive_fraction"
                                }
                            }

                            Column {
                                width: parent.width
                                spacing: 8
                                visible: window.derivedMetricDraftKind === "positive_fraction"

                                ComboBox {
                                    width: parent.width
                                    model: desktopController.sample.channels || []
                                    currentIndex: window.listIndex(model, window.derivedMetricDraftChannel)
                                    onActivated: window.derivedMetricDraftChannel = model[currentIndex] || ""
                                }

                                TextField {
                                    id: derivedMetricThresholdField
                                    width: parent.width
                                    text: window.derivedMetricDraftThreshold
                                    placeholderText: "Threshold"
                                    selectByMouse: true
                                    onTextEdited: window.derivedMetricDraftThreshold = text
                                }

                                Button {
                                    text: "Apply Positive Fraction"
                                    enabled: window.derivedMetricDraftChannel !== ""
                                    onClicked: desktopController.setDerivedMetricPositiveFraction(
                                                   window.derivedMetricDraftChannel,
                                                   Number(derivedMetricThresholdField.text))
                                }
                            }

                            Column {
                                width: parent.width
                                spacing: 8
                                visible: window.derivedMetricDraftKind === "mean_ratio"

                                ComboBox {
                                    width: parent.width
                                    model: desktopController.sample.channels || []
                                    currentIndex: window.listIndex(model, window.derivedMetricDraftNumeratorChannel)
                                    onActivated: window.derivedMetricDraftNumeratorChannel = model[currentIndex] || ""
                                }

                                ComboBox {
                                    width: parent.width
                                    model: desktopController.sample.channels || []
                                    currentIndex: window.listIndex(model, window.derivedMetricDraftDenominatorChannel)
                                    onActivated: window.derivedMetricDraftDenominatorChannel = model[currentIndex] || ""
                                }

                                Button {
                                    text: "Apply Mean Ratio"
                                    enabled: window.derivedMetricDraftNumeratorChannel !== ""
                                             && window.derivedMetricDraftDenominatorChannel !== ""
                                    onClicked: desktopController.setDerivedMetricMeanRatio(
                                                   window.derivedMetricDraftNumeratorChannel,
                                                   window.derivedMetricDraftDenominatorChannel)
                                }
                            }
                        }

                        Column {
                            width: parent.width
                            spacing: 10

                            Text {
                                text: "Analysis Settings"
                                color: "#2e2216"
                                font.pixelSize: 22
                                font.weight: Font.DemiBold
                            }

                            CheckBox {
                                text: desktopController.sample.compensation_available
                                      ? "Apply Parsed Compensation"
                                      : "No Compensation Matrix In Sample"
                                enabled: desktopController.sample.compensation_available || false
                                checked: desktopController.sample.compensation_enabled || false
                                onClicked: desktopController.setCompensationEnabled(checked)
                            }

                            Text {
                                width: parent.width
                                text: desktopController.sample.compensation_source_key
                                      ? "Source: " + desktopController.sample.compensation_source_key
                                      : "Transforms and compensation are replayed before every gate redraw."
                                color: "#6d5941"
                                font.pixelSize: 13
                                wrapMode: Text.WordWrap
                            }

                            Repeater {
                                model: desktopController.sample.channel_transforms || []

                                delegate: Row {
                                    width: parent.width
                                    spacing: 10

                                    Text {
                                        width: 120
                                        text: modelData.channel
                                        color: "#2e2216"
                                        font.pixelSize: 13
                                        elide: Text.ElideRight
                                    }

                                    ComboBox {
                                        model: [
                                            "Linear",
                                            "Signed Log10",
                                            "Asinh (150)",
                                            "Biexponential",
                                            "Logicle"
                                        ]
                                        currentIndex: window.transformIndex(modelData.kind || "linear")
                                        onActivated: desktopController.setChannelTransform(
                                                         modelData.channel,
                                                         window.transformKindAt(currentIndex))
                                    }
                                }
                            }
                        }

                        Column {
                            width: parent.width
                            spacing: 10

                            Text {
                                text: "Cross-Sample Comparison"
                                color: "#2e2216"
                                font.pixelSize: 22
                                font.weight: Font.DemiBold
                            }

                            Text {
                                width: parent.width
                                text: desktopController.samples.length > 1
                                      ? "Comparing "
                                        + (desktopController.selectedPopulationComparison.population_id
                                           || desktopController.selectedPopulationStats.population_id
                                           || "All Events")
                                        + " across "
                                        + (desktopController.selectedPopulationComparison.available_sample_count || 0)
                                        + " of "
                                        + desktopController.samples.length
                                        + " loaded samples."
                                      : "Load more than one sample to compare the selected population side by side."
                                color: "#6d5941"
                                font.pixelSize: 13
                                wrapMode: Text.WordWrap
                            }

                            Repeater {
                                model: desktopController.selectedPopulationComparison.samples || []

                                delegate: Rectangle {
                                    width: parent.width
                                    radius: 12
                                    color: modelData.is_active_sample ? "#e7f0eb"
                                          : modelData.status === "available" ? "#f7f4ed" : "#f7ede8"
                                    border.width: 1
                                    border.color: modelData.is_active_sample ? "#9fbea9"
                                                : modelData.status === "available" ? "#d8ccb7" : "#dfb9a4"
                                    implicitHeight: comparisonCardContent.implicitHeight + 24

                                    Column {
                                        id: comparisonCardContent
                                        anchors.fill: parent
                                        anchors.margins: 12
                                        spacing: 4

                                        Text {
                                            text: modelData.display_name
                                                  + (modelData.is_active_sample ? "  •  Active baseline" : "")
                                            color: "#2e2216"
                                            font.pixelSize: 15
                                            font.weight: Font.Medium
                                        }

                                        Text {
                                            text: "Group: " + (modelData.group_label || "Ungrouped")
                                            color: "#6d5941"
                                            font.pixelSize: 13
                                        }

                                        Text {
                                            visible: modelData.status === "available"
                                            text: "Events " + (modelData.matched_events || 0)
                                                  + " of " + (modelData.parent_events || 0)
                                                  + "  •  All " + window.formatPercent(modelData.frequency_of_all)
                                                  + "  •  Parent " + window.formatPercent(modelData.frequency_of_parent)
                                            color: "#6d5941"
                                            font.pixelSize: 13
                                            wrapMode: Text.WordWrap
                                        }

                                        Text {
                                            visible: modelData.status === "available" && !modelData.is_active_sample
                                            text: "Delta vs active: all "
                                                  + window.formatSignedPercent(modelData.delta_frequency_of_all)
                                                  + "  •  parent "
                                                  + window.formatSignedPercent(modelData.delta_frequency_of_parent)
                                            color: "#6d5941"
                                            font.pixelSize: 13
                                            wrapMode: Text.WordWrap
                                        }

                                        Text {
                                            visible: modelData.status === "available"
                                                     && modelData.derived_metric_status === "available"
                                            text: window.derivedMetricLabel() + "  •  "
                                                  + window.formatDerivedMetricValue(
                                                      modelData.derived_metric_value,
                                                      desktopController.derivedMetric.kind)
                                            color: "#6d5941"
                                            font.pixelSize: 13
                                            wrapMode: Text.WordWrap
                                        }

                                        Text {
                                            visible: modelData.status === "available"
                                                     && modelData.derived_metric_status === "available"
                                                     && !modelData.is_active_sample
                                            text: "Derived delta vs active: "
                                                  + window.formatSignedDerivedMetricValue(
                                                      modelData.derived_metric_delta_value,
                                                      desktopController.derivedMetric.kind)
                                            color: "#6d5941"
                                            font.pixelSize: 13
                                            wrapMode: Text.WordWrap
                                        }

                                        Text {
                                            visible: modelData.status === "available"
                                                     && modelData.derived_metric_status !== "available"
                                                     && (modelData.derived_metric_message || "") !== ""
                                            text: modelData.derived_metric_message
                                            color: "#7a5947"
                                            font.pixelSize: 13
                                            wrapMode: Text.WordWrap
                                        }

                                        Text {
                                            visible: modelData.status !== "available"
                                            text: "This population is not present in the current gate history for this sample yet."
                                            color: "#7a5947"
                                            font.pixelSize: 13
                                            wrapMode: Text.WordWrap
                                        }
                                    }
                                }
                            }
                        }

                        Column {
                            width: parent.width
                            spacing: 10

                            Text {
                                text: "Cohort Summary"
                                color: "#2e2216"
                                font.pixelSize: 22
                                font.weight: Font.DemiBold
                            }

                            Text {
                                width: parent.width
                                text: desktopController.samples.length > 1
                                      ? "Group labels turn the selected-population comparison into condition-aware cohort summaries."
                                      : "Load more than one sample to summarize groups."
                                color: "#6d5941"
                                font.pixelSize: 13
                                wrapMode: Text.WordWrap
                            }

                            Repeater {
                                model: desktopController.selectedPopulationComparison.group_summaries || []

                                delegate: Rectangle {
                                    width: parent.width
                                    radius: 12
                                    color: modelData.is_active_group ? "#eef3f0" : "#f7f4ed"
                                    border.width: 1
                                    border.color: modelData.is_active_group ? "#bfd0c5" : "#d8ccb7"
                                    implicitHeight: cohortCardContent.implicitHeight + 24

                                    Column {
                                        id: cohortCardContent
                                        anchors.fill: parent
                                        anchors.margins: 12
                                        spacing: 4

                                        Text {
                                            text: modelData.group_label
                                                  + (modelData.is_active_group ? "  •  Active cohort" : "")
                                            color: "#2e2216"
                                            font.pixelSize: 15
                                            font.weight: Font.Medium
                                        }

                                        Text {
                                            text: (modelData.available_sample_count || 0)
                                                  + " of " + (modelData.sample_count || 0)
                                                  + " samples available"
                                                  + ((modelData.missing_sample_count || 0) > 0
                                                     ? "  •  " + modelData.missing_sample_count + " missing"
                                                     : "")
                                            color: "#6d5941"
                                            font.pixelSize: 13
                                            wrapMode: Text.WordWrap
                                        }

                                        Text {
                                            text: (modelData.available_sample_count || 0) > 0
                                                  ? "Derived metric coverage "
                                                    + (modelData.derived_metric_available_sample_count || 0)
                                                    + " of " + (modelData.available_sample_count || 0)
                                                    + " comparable samples"
                                                    + ((modelData.derived_metric_unavailable_sample_count || 0) > 0
                                                       ? "  •  "
                                                         + modelData.derived_metric_unavailable_sample_count
                                                         + " unavailable"
                                                       : "")
                                                  : "Derived metric coverage n/a until this cohort has the selected population"
                                            color: "#6d5941"
                                            font.pixelSize: 13
                                            wrapMode: Text.WordWrap
                                        }

                                        Text {
                                            text: "Mean of all " + window.formatPercentOrNA(modelData.mean_frequency_of_all)
                                                  + "  •  Mean of parent " + window.formatPercentOrNA(modelData.mean_frequency_of_parent)
                                            color: "#6d5941"
                                            font.pixelSize: 13
                                            wrapMode: Text.WordWrap
                                        }

                                        Text {
                                            text: window.derivedMetricLabel() + " mean "
                                                  + window.formatDerivedMetricValue(
                                                      modelData.mean_derived_metric_value,
                                                      desktopController.derivedMetric.kind)
                                            color: "#6d5941"
                                            font.pixelSize: 13
                                            wrapMode: Text.WordWrap
                                        }

                                        Text {
                                            visible: !modelData.is_active_group
                                            text: "Delta vs active cohort: all "
                                                  + window.formatSignedPercent(modelData.delta_mean_frequency_of_all)
                                                  + "  •  parent "
                                                  + window.formatSignedPercent(modelData.delta_mean_frequency_of_parent)
                                            color: "#6d5941"
                                            font.pixelSize: 13
                                            wrapMode: Text.WordWrap
                                        }

                                        Text {
                                            visible: !modelData.is_active_group
                                            text: "Derived delta vs active cohort: "
                                                  + window.formatSignedDerivedMetricValue(
                                                      modelData.delta_mean_derived_metric_value,
                                                      desktopController.derivedMetric.kind)
                                            color: "#6d5941"
                                            font.pixelSize: 13
                                            wrapMode: Text.WordWrap
                                        }
                                    }
                                }
                            }
                        }

                        Column {
                            width: parent.width
                            spacing: 10

                            Text {
                                text: "Command Presets"
                                color: "#2e2216"
                                font.pixelSize: 22
                                font.weight: Font.DemiBold
                            }

                            Button {
                                text: desktopController.hasPopulation("lymphocytes")
                                      ? "Lymphocyte Gate Added"
                                      : "Add Lymphocyte Gate"
                                enabled: !desktopController.hasPopulation("lymphocytes")
                                         && desktopController.canApplyPreset("lymphocytes")
                                onClicked: desktopController.applyPresetCommand("lymphocytes")
                            }

                            Button {
                                text: desktopController.hasPopulation("cd3_cd4")
                                      ? "CD3/CD4 Gate Added"
                                      : "Add CD3/CD4 Gate"
                                enabled: !desktopController.hasPopulation("cd3_cd4")
                                         && desktopController.hasPopulation("lymphocytes")
                                         && desktopController.canApplyPreset("cd3_cd4")
                                onClicked: desktopController.applyPresetCommand("cd3_cd4")
                            }

                            Button {
                                text: "Reset Session"
                                onClicked: desktopController.resetSession()
                            }

                            Row {
                                spacing: 10

                                Button {
                                    text: "Rectangle Tool"
                                    checkable: true
                                    checked: window.activeGateTool === "rectangle"
                                    onClicked: window.activeGateTool = "rectangle"
                                }

                                Button {
                                    text: "Polygon Tool"
                                    checkable: true
                                    checked: window.activeGateTool === "polygon"
                                    onClicked: window.activeGateTool = "polygon"
                                }
                            }

                            Row {
                                spacing: 10

                                Button {
                                    text: "Undo"
                                    enabled: desktopController.canUndo
                                    onClicked: desktopController.undo()
                                }

                                Button {
                                    text: "Redo"
                                    enabled: desktopController.canRedo
                                    onClicked: desktopController.redo()
                                }
                            }

                            Text {
                                width: parent.width
                                text: window.activeGateTool === "rectangle"
                                      ? "Drag directly on either plot to create a rectangle gate. The new gate is appended to the Rust command log and becomes a child of the currently selected population."
                                      : "Click to place polygon vertices on either plot, then right-click to commit. Right-click with fewer than three vertices clears the draft."
                                color: "#6d5941"
                                font.pixelSize: 13
                                wrapMode: Text.WordWrap
                            }

                            Text {
                                width: parent.width
                                text: desktopController.sample.source_path
                                      ? "Active file: " + desktopController.sample.source_path
                                      : "Active file: bundled desktop demo sample"
                                color: "#8a7354"
                                font.pixelSize: 12
                                wrapMode: Text.WordWrap
                            }
                        }

                        Column {
                            width: parent.width
                            spacing: 10

                            Text {
                                text: "Analysis History"
                                color: "#2e2216"
                                font.pixelSize: 22
                                font.weight: Font.DemiBold
                            }

                            Repeater {
                                model: desktopController.analysisActions

                                delegate: Rectangle {
                                    width: parent.width
                                    radius: 12
                                    color: "#eef3f0"
                                    border.width: 1
                                    border.color: "#bfd0c5"
                                    implicitHeight: 58

                                    Column {
                                        anchors.fill: parent
                                        anchors.margins: 12
                                        spacing: 4

                                        Text {
                                            text: modelData.sequence + ". " + modelData.kind
                                            color: "#214034"
                                            font.pixelSize: 15
                                            font.weight: Font.Medium
                                        }

                                        Text {
                                            text: modelData.summary || ""
                                            color: "#51685c"
                                            font.pixelSize: 13
                                        }
                                    }
                                }
                            }
                        }

                        Column {
                            width: parent.width
                            spacing: 10

                            Text {
                                text: "Populations"
                                color: "#2e2216"
                                font.pixelSize: 22
                                font.weight: Font.DemiBold
                            }

                            Repeater {
                                model: desktopController.populations

                                delegate: Rectangle {
                                    width: parent.width
                                    radius: 14
                                    color: modelData.key === desktopController.selectedPopulationKey
                                           ? "#e5d3ac"
                                           : "#f6efe1"
                                    border.width: 1
                                    border.color: modelData.key === desktopController.selectedPopulationKey
                                                  ? "#9a7b3f"
                                                  : "#dcc8a0"
                                    implicitHeight: 62

                                    MouseArea {
                                        anchors.fill: parent
                                        onClicked: desktopController.selectedPopulationKey = modelData.key
                                    }

                                    Column {
                                        anchors.fill: parent
                                        anchors.margins: 12
                                        spacing: 4

                                        Text {
                                            text: modelData.population_id
                                            color: "#2e2216"
                                            font.pixelSize: 16
                                            font.weight: Font.DemiBold
                                        }

                                        Text {
                                            text: "Matched events: " + modelData.matched_events
                                            color: "#6d5941"
                                            font.pixelSize: 13
                                        }
                                    }
                                }
                            }
                        }

                        Column {
                            width: parent.width
                            spacing: 10

                            Text {
                                text: "Population Stats"
                                color: "#2e2216"
                                font.pixelSize: 22
                                font.weight: Font.DemiBold
                            }

                            Rectangle {
                                width: parent.width
                                radius: 14
                                color: "#eef3f0"
                                border.width: 1
                                border.color: "#bfd0c5"
                                implicitHeight: 110

                                Column {
                                    anchors.fill: parent
                                    anchors.margins: 12
                                    spacing: 6

                                    Text {
                                        text: desktopController.selectedPopulationStats.population_id || "All Events"
                                        color: "#214034"
                                        font.pixelSize: 16
                                        font.weight: Font.DemiBold
                                    }

                                    Text {
                                        text: "Events " + (desktopController.selectedPopulationStats.matched_events || 0)
                                              + " of " + (desktopController.selectedPopulationStats.parent_events || 0)
                                        color: "#51685c"
                                        font.pixelSize: 13
                                    }

                                    Text {
                                        text: "Of all events " + window.formatPercent(
                                                  desktopController.selectedPopulationStats.frequency_of_all)
                                        color: "#51685c"
                                        font.pixelSize: 13
                                    }

                                    Text {
                                        text: "Of parent " + window.formatPercent(
                                                  desktopController.selectedPopulationStats.frequency_of_parent)
                                        color: "#51685c"
                                        font.pixelSize: 13
                                    }
                                }
                            }

                            Repeater {
                                model: desktopController.selectedPopulationStats.channel_stats || []

                                delegate: Rectangle {
                                    width: parent.width
                                    radius: 12
                                    color: "#f7f4ed"
                                    border.width: 1
                                    border.color: "#d8ccb7"
                                    implicitHeight: 62

                                    Column {
                                        anchors.fill: parent
                                        anchors.margins: 12
                                        spacing: 4

                                        Text {
                                            text: modelData.channel || ""
                                            color: "#2e2216"
                                            font.pixelSize: 15
                                            font.weight: Font.Medium
                                        }

                                        Text {
                                            text: "Mean " + window.formatStatValue(modelData.mean)
                                                  + "  •  Median " + window.formatStatValue(modelData.median)
                                            color: "#6d5941"
                                            font.pixelSize: 13
                                        }
                                    }
                                }
                            }
                        }

                        Column {
                            width: parent.width
                            spacing: 10

                            Text {
                                text: "Command Log"
                                color: "#2e2216"
                                font.pixelSize: 22
                                font.weight: Font.DemiBold
                            }

                            Repeater {
                                model: desktopController.commands

                                delegate: Rectangle {
                                    width: parent.width
                                    radius: 12
                                    color: "#f6efe1"
                                    border.width: 1
                                    border.color: "#dcc8a0"
                                    implicitHeight: 58

                                    Column {
                                        anchors.fill: parent
                                        anchors.margins: 12
                                        spacing: 4

                                        Text {
                                            text: modelData.sequence + ". " + modelData.kind
                                            color: "#2e2216"
                                            font.pixelSize: 15
                                            font.weight: Font.Medium
                                        }

                                        Text {
                                            text: modelData.population_id
                                            color: "#6d5941"
                                            font.pixelSize: 13
                                        }
                                    }
                                }
                            }
                        }

                        Rectangle {
                            width: parent.width
                            radius: 14
                            color: desktopController.lastError === "" ? "#edf4ee" : "#f8ddd3"
                            border.width: 1
                            border.color: desktopController.lastError === "" ? "#b8d2bb" : "#dd8a70"
                            implicitHeight: 92

                            Column {
                                anchors.fill: parent
                                anchors.margins: 14
                                spacing: 6

                                Text {
                                    text: "Bridge Feedback"
                                    color: "#2e2216"
                                    font.pixelSize: 17
                                    font.weight: Font.DemiBold
                                }

                                Text {
                                    width: parent.width
                                    text: desktopController.lastError === ""
                                          ? "Rust session is ready for explicit user-driven commands."
                                          : desktopController.lastError
                                    color: "#594836"
                                    font.pixelSize: 14
                                    wrapMode: Text.WordWrap
                                }
                            }
                        }
                    }
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: 18

                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 1
                    Layout.fillHeight: true
                    radius: 22
                    color: "#fffaf1"
                    border.width: 1
                    border.color: "#dcc8a0"

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 18
                        spacing: 10

                        RowLayout {
                            Layout.fillWidth: true

                            Text {
                                text: plotA.title || "Plot A"
                                color: "#2e2216"
                                font.pixelSize: 22
                                font.weight: Font.DemiBold
                            }

                            Item { Layout.fillWidth: true }

                            Text {
                                text: "Highlighted " + (plotA.highlight_count || 0)
                                color: "#6d5941"
                                font.pixelSize: 14
                            }
                        }

                        Text {
                            text: window.plotAxisLabel(plotA)
                            color: "#6d5941"
                            font.pixelSize: 14
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8

                            Button {
                                text: "Auto"
                                onClicked: desktopController.resetPlotView(plotA.id || "")
                            }

                            Button {
                                text: "Focus"
                                onClicked: desktopController.focusPlotOnSelectedPopulation(plotA.id || "")
                            }

                            Button {
                                text: "Zoom In"
                                onClicked: desktopController.scalePlotView(plotA.id || "", 0.7)
                            }

                            Button {
                                text: "Zoom Out"
                                onClicked: desktopController.scalePlotView(plotA.id || "", 1.4)
                            }

                            Item { Layout.fillWidth: true }

                            Text {
                                text: plotA.view_summary || "Auto extents"
                                color: "#8b6a3c"
                                font.pixelSize: 13
                            }
                        }

                        Text {
                            text: window.plotHelperText(plotA)
                            color: "#8b6a3c"
                            font.pixelSize: 13
                        }

                        Rectangle {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            radius: 18
                            color: "#f2eadc"
                            border.width: 1
                            border.color: "#d3c2a0"

                            ScatterPlotItem {
                                anchors.fill: parent
                                anchors.margins: 10
                                visible: (plotA.kind || "scatter") !== "histogram"
                                allPoints: plotA.all_points || []
                                highlightPoints: plotA.highlight_points || []
                                xMin: plotA.x_range ? plotA.x_range.min : 0
                                xMax: plotA.x_range ? plotA.x_range.max : 1
                                yMin: plotA.y_range ? plotA.y_range.min : 0
                                yMax: plotA.y_range ? plotA.y_range.max : 1
                                interactionMode: window.activeGateTool
                                onRectangleGateDrawn: function (xMin, xMax, yMin, yMax) {
                                    desktopController.createRectangleGateForPlot(
                                                plotA.id || "",
                                                xMin,
                                                xMax,
                                                yMin,
                                                yMax)
                                }
                                onPolygonGateDrawn: function (vertices) {
                                    desktopController.createPolygonGateForPlot(
                                                plotA.id || "",
                                                vertices)
                                }
                            }

                            HistogramPlotItem {
                                anchors.fill: parent
                                anchors.margins: 10
                                visible: (plotA.kind || "") === "histogram"
                                allBins: plotA.all_bins || []
                                highlightBins: plotA.highlight_bins || []
                                xMin: plotA.x_range ? plotA.x_range.min : 0
                                xMax: plotA.x_range ? plotA.x_range.max : 1
                                yMin: plotA.y_range ? plotA.y_range.min : 0
                                yMax: plotA.y_range ? plotA.y_range.max : 1
                            }
                        }
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 1
                    Layout.fillHeight: true
                    radius: 22
                    color: "#fffaf1"
                    border.width: 1
                    border.color: "#dcc8a0"

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 18
                        spacing: 10

                        RowLayout {
                            Layout.fillWidth: true

                            Text {
                                text: plotB.title || "Plot B"
                                color: "#2e2216"
                                font.pixelSize: 22
                                font.weight: Font.DemiBold
                            }

                            Item { Layout.fillWidth: true }

                            Text {
                                text: "Highlighted " + (plotB.highlight_count || 0)
                                color: "#6d5941"
                                font.pixelSize: 14
                            }
                        }

                        Text {
                            text: window.plotAxisLabel(plotB)
                            color: "#6d5941"
                            font.pixelSize: 14
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8

                            Button {
                                text: "Auto"
                                onClicked: desktopController.resetPlotView(plotB.id || "")
                            }

                            Button {
                                text: "Focus"
                                onClicked: desktopController.focusPlotOnSelectedPopulation(plotB.id || "")
                            }

                            Button {
                                text: "Zoom In"
                                onClicked: desktopController.scalePlotView(plotB.id || "", 0.7)
                            }

                            Button {
                                text: "Zoom Out"
                                onClicked: desktopController.scalePlotView(plotB.id || "", 1.4)
                            }

                            Item { Layout.fillWidth: true }

                            Text {
                                text: plotB.view_summary || "Auto extents"
                                color: "#8b6a3c"
                                font.pixelSize: 13
                            }
                        }

                        Text {
                            text: window.plotHelperText(plotB)
                            color: "#8b6a3c"
                            font.pixelSize: 13
                        }

                        Rectangle {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            radius: 18
                            color: "#f2eadc"
                            border.width: 1
                            border.color: "#d3c2a0"

                            ScatterPlotItem {
                                anchors.fill: parent
                                anchors.margins: 10
                                visible: (plotB.kind || "scatter") !== "histogram"
                                allPoints: plotB.all_points || []
                                highlightPoints: plotB.highlight_points || []
                                xMin: plotB.x_range ? plotB.x_range.min : 0
                                xMax: plotB.x_range ? plotB.x_range.max : 1
                                yMin: plotB.y_range ? plotB.y_range.min : 0
                                yMax: plotB.y_range ? plotB.y_range.max : 1
                                interactionMode: window.activeGateTool
                                onRectangleGateDrawn: function (xMin, xMax, yMin, yMax) {
                                    desktopController.createRectangleGateForPlot(
                                                plotB.id || "",
                                                xMin,
                                                xMax,
                                                yMin,
                                                yMax)
                                }
                                onPolygonGateDrawn: function (vertices) {
                                    desktopController.createPolygonGateForPlot(
                                                plotB.id || "",
                                                vertices)
                                }
                            }

                            HistogramPlotItem {
                                anchors.fill: parent
                                anchors.margins: 10
                                visible: (plotB.kind || "") === "histogram"
                                allBins: plotB.all_bins || []
                                highlightBins: plotB.highlight_bins || []
                                xMin: plotB.x_range ? plotB.x_range.min : 0
                                xMax: plotB.x_range ? plotB.x_range.max : 1
                                yMin: plotB.y_range ? plotB.y_range.min : 0
                                yMax: plotB.y_range ? plotB.y_range.max : 1
                            }
                        }
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 1
                    Layout.fillHeight: true
                    visible: !!(plotC.id || "")
                    radius: 22
                    color: "#fffaf1"
                    border.width: 1
                    border.color: "#dcc8a0"

                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 18
                        spacing: 10

                        RowLayout {
                            Layout.fillWidth: true

                            Text {
                                text: plotC.title || "Plot C"
                                color: "#2e2216"
                                font.pixelSize: 22
                                font.weight: Font.DemiBold
                            }

                            Item { Layout.fillWidth: true }

                            Text {
                                text: "Highlighted " + (plotC.highlight_count || 0)
                                color: "#6d5941"
                                font.pixelSize: 14
                            }
                        }

                        Text {
                            text: window.plotAxisLabel(plotC)
                            color: "#6d5941"
                            font.pixelSize: 14
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 8

                            Button {
                                text: "Auto"
                                onClicked: desktopController.resetPlotView(plotC.id || "")
                            }

                            Button {
                                text: "Focus"
                                onClicked: desktopController.focusPlotOnSelectedPopulation(plotC.id || "")
                            }

                            Button {
                                text: "Zoom In"
                                onClicked: desktopController.scalePlotView(plotC.id || "", 0.7)
                            }

                            Button {
                                text: "Zoom Out"
                                onClicked: desktopController.scalePlotView(plotC.id || "", 1.4)
                            }

                            Item { Layout.fillWidth: true }

                            Text {
                                text: plotC.view_summary || "Auto extents"
                                color: "#8b6a3c"
                                font.pixelSize: 13
                            }
                        }

                        Text {
                            text: window.plotHelperText(plotC)
                            color: "#8b6a3c"
                            font.pixelSize: 13
                        }

                        Rectangle {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            radius: 18
                            color: "#f2eadc"
                            border.width: 1
                            border.color: "#d3c2a0"

                            ScatterPlotItem {
                                anchors.fill: parent
                                anchors.margins: 10
                                visible: (plotC.kind || "scatter") !== "histogram"
                                allPoints: plotC.all_points || []
                                highlightPoints: plotC.highlight_points || []
                                xMin: plotC.x_range ? plotC.x_range.min : 0
                                xMax: plotC.x_range ? plotC.x_range.max : 1
                                yMin: plotC.y_range ? plotC.y_range.min : 0
                                yMax: plotC.y_range ? plotC.y_range.max : 1
                                interactionMode: window.activeGateTool
                                onRectangleGateDrawn: function (xMin, xMax, yMin, yMax) {
                                    desktopController.createRectangleGateForPlot(
                                                plotC.id || "",
                                                xMin,
                                                xMax,
                                                yMin,
                                                yMax)
                                }
                                onPolygonGateDrawn: function (vertices) {
                                    desktopController.createPolygonGateForPlot(
                                                plotC.id || "",
                                                vertices)
                                }
                            }

                            HistogramPlotItem {
                                anchors.fill: parent
                                anchors.margins: 10
                                visible: (plotC.kind || "") === "histogram"
                                allBins: plotC.all_bins || []
                                highlightBins: plotC.highlight_bins || []
                                xMin: plotC.x_range ? plotC.x_range.min : 0
                                xMax: plotC.x_range ? plotC.x_range.max : 1
                                yMin: plotC.y_range ? plotC.y_range.min : 0
                                yMax: plotC.y_range ? plotC.y_range.max : 1
                            }
                        }
                    }
                }
            }
        }
    }
}
