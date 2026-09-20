pragma ComponentBehavior: Bound

import QtQuick
import QtTest

Item {
    id: root
    width: 960
    height: 720

    ListModel {
        id: tracks
    }

    Component.onCompleted: {
        for (var i = 0; i < 2000; i++) {
            tracks.append({
                title: "曲目 " + i,
                trackId: i
            })
        }
    }

    ListView {
        id: list
        anchors.fill: parent
        clip: true
        reuseItems: true
        cacheBuffer: 580
        boundsBehavior: Flickable.StopAtBounds
        model: tracks
        spacing: 2
        delegate: Rectangle {
            id: row
            required property int trackId
            required property string title
            readonly property bool liveRow: true
            width: ListView.view ? ListView.view.width : 0
            height: 58
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: row.title
            }
        }
    }

    function liveDelegates() {
        var count = 0
        var children = list.contentItem.children
        for (var i = 0; i < children.length; i++) {
            if (children[i].liveRow === true)
                count++
        }
        return count
    }

    TestCase {
        name: "VirtualizedTrackList"
        when: windowShown

        function test_delegate_count_tracks_viewport_not_model() {
            wait(50)
            var live = root.liveDelegates()
            verify(list.count === 2000)
            verify(live > 0)
            verify(live < 80)
            console.log("virtualization model=" + list.count + " delegates=" + live)
        }
    }
}
