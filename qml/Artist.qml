pragma ComponentBehavior: Bound

import QtQuick

CollectionBrowser {
    id: root

    required property var libraryModel

    title: "歌手"
    emptyTitle: "还没有歌手"
    emptySubtitle: "添加音乐后，歌手将显示在这里"
    collectionModel: root.libraryModel.artist_model
    detailModel: root.libraryModel.artist_detail
    selectedName: root.libraryModel.selected_artist
    selectedSubtitle: root.libraryModel.selected_artist_subtitle
    selectedCover: root.libraryModel.selected_artist_cover
    onCollectionOpened: function(collectionId) { root.libraryModel.open_artist(collectionId) }
    onCollectionClosed: root.libraryModel.close_artist()
    onTrackActivated: function(trackId) { root.libraryModel.play_artist_track(trackId) }
}
