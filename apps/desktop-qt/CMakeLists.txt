cmake_minimum_required(VERSION 3.24)
project(flowjoish-desktop LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTOUIC ON)
set(CMAKE_AUTORCC ON)

if(NOT CMAKE_PREFIX_PATH)
    execute_process(
        COMMAND qmake -query QT_INSTALL_PREFIX
        OUTPUT_VARIABLE QT_PREFIX
        OUTPUT_STRIP_TRAILING_WHITESPACE
        ERROR_QUIET
    )
    if(QT_PREFIX)
        list(APPEND CMAKE_PREFIX_PATH "${QT_PREFIX}")
    endif()
endif()

find_package(QT NAMES Qt6 Qt5 REQUIRED COMPONENTS Core Gui Qml Quick QuickControls2 Widgets)
find_package(Qt${QT_VERSION_MAJOR} REQUIRED COMPONENTS Core Gui Qml Quick QuickControls2 Widgets)

if(CMAKE_BUILD_TYPE STREQUAL "Release")
    set(FLOWJOISH_RUST_PROFILE release)
    set(FLOWJOISH_RUST_ARGS --release)
else()
    set(FLOWJOISH_RUST_PROFILE debug)
    set(FLOWJOISH_RUST_ARGS)
endif()

set(FLOWJOISH_WORKSPACE_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../..")
set(FLOWJOISH_RUST_LIB
    "${FLOWJOISH_WORKSPACE_ROOT}/target/${FLOWJOISH_RUST_PROFILE}/${CMAKE_STATIC_LIBRARY_PREFIX}flowjoish_desktop_bridge${CMAKE_STATIC_LIBRARY_SUFFIX}"
)

add_custom_target(flowjoish-rust-bridge ALL
    BYPRODUCTS "${FLOWJOISH_RUST_LIB}"
    COMMAND cargo build -p flowjoish-desktop-bridge ${FLOWJOISH_RUST_ARGS}
    WORKING_DIRECTORY "${FLOWJOISH_WORKSPACE_ROOT}"
    COMMENT "Building Rust desktop bridge"
    VERBATIM
)

add_executable(flowjoish-desktop
    src/DesktopController.cpp
    src/DesktopController.h
    src/DesktopPayloadPolicy.cpp
    src/DesktopPayloadPolicy.h
    src/HistogramPlotItem.cpp
    src/HistogramPlotItem.h
    src/ScatterPlotItem.cpp
    src/ScatterPlotItem.h
    src/main.cpp
)

add_dependencies(flowjoish-desktop flowjoish-rust-bridge)

target_compile_definitions(flowjoish-desktop PRIVATE FLOWJOISH_QML_DIR="${CMAKE_CURRENT_SOURCE_DIR}/qml")
target_link_libraries(flowjoish-desktop PRIVATE
    Qt${QT_VERSION_MAJOR}::Core
    Qt${QT_VERSION_MAJOR}::Gui
    Qt${QT_VERSION_MAJOR}::Qml
    Qt${QT_VERSION_MAJOR}::Quick
    Qt${QT_VERSION_MAJOR}::QuickControls2
    Qt${QT_VERSION_MAJOR}::Widgets
    "${FLOWJOISH_RUST_LIB}"
)

enable_testing()

add_executable(flowjoish-desktop-policy-tests
    src/DesktopPayloadPolicy.cpp
    src/DesktopPayloadPolicy.h
    tests/DesktopPayloadPolicyTests.cpp
)

target_link_libraries(flowjoish-desktop-policy-tests PRIVATE
    Qt${QT_VERSION_MAJOR}::Core
)

add_test(NAME flowjoish-desktop-policy-tests COMMAND flowjoish-desktop-policy-tests)
