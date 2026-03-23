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
    property string activeGateTool: "rectangle"

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
                                text: desktopController.workspacePath === ""
                                      ? "Workspace: not saved yet"
                                      : "Workspace: " + desktopController.workspacePath
                                color: "#8a7354"
                                font.pixelSize: 12
                                wrapMode: Text.WordWrap
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
                                    implicitHeight: 76

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
                            text: (plotA.x_channel || "x") + " vs " + (plotA.y_channel || "y")
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
                            text: window.activeGateTool === "rectangle"
                                  ? "Drag to author a rectangle gate on this projection"
                                  : "Click to place polygon vertices, then right-click to finish"
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
                            text: (plotB.x_channel || "x") + " vs " + (plotB.y_channel || "y")
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
                            text: window.activeGateTool === "rectangle"
                                  ? "Drag to author a rectangle gate on this projection"
                                  : "Click to place polygon vertices, then right-click to finish"
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
                        }
                    }
                }
            }
        }
    }
}
