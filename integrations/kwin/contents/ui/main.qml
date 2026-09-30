import QtQuick
import org.kde.kwin

Item {
    id: helper
    function report() {
        const windows = workspace.windowList();
        const full = windows.some(w => w.fullScreen && !w.minimized &&
            (w.onAllDesktops || w.desktops.indexOf(workspace.currentDesktop) !== -1));
        reportCall.arguments = [full, false, "KDE Plasma"];
        reportCall.call();
    }
    function decorate(window) {
        if (String(window.resourceClass).toLowerCase() === "adhd") {
            window.keepAbove = true;
            window.onAllDesktops = true;
        }
    }
    DBusCall {
        id: reportCall
        service: "io.github.adhd.Desktop"
        path: "/io/github/adhd/Desktop"
        dbusInterface: "io.github.adhd.Desktop"
        method: "Report"
    }
    Timer { interval: 2000; running: true; repeat: true; onTriggered: helper.report() }
    Connections {
        target: workspace
        function onWindowAdded(window) { helper.decorate(window); helper.report(); }
        function onWindowRemoved() { helper.report(); }
        function onWindowActivated() { helper.report(); }
        function onCurrentDesktopChanged() { helper.report(); }
    }
    Component.onCompleted: {
        workspace.windowList().forEach(decorate);
        report();
    }
}
