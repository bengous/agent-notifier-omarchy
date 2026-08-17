import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "AgentNotifier" as Plugin

ShellRoot {
  id: root

  // Omarchy's Bar cannot be instantiated outside omarchy-shell, so the harness
  // supplies the three members PopupCard and BarWidget read off their host.
  QtObject {
    id: harnessBar

    property string position: "top"
    property bool vertical: false
    property int barSize: Style.bar.sizeHorizontal
    property var activePopout: null

    function requestPopout(key) { activePopout = key }
    function releasePopout(key) { if (activePopout === key) activePopout = null }
  }

  IpcHandler {
    target: "harness"

    function probe(): string {
      return JSON.stringify({
        events: widget.events.length,
        unread: widget.unreadCount,
        popupOpen: widget.popupOpen,
        cliMissing: widget.cliMissing
      })
    }
  }

  PanelWindow {
    anchors {
      top: true
      left: true
      right: true
    }
    implicitHeight: Style.bar.sizeHorizontal
    color: Color.bar.background

    Plugin.BarWidget {
      id: widget
      bar: harnessBar
      anchors.left: parent.left
      anchors.leftMargin: Style.space(12)
      anchors.verticalCenter: parent.verticalCenter
    }
  }
}
