pragma ComponentBehavior: Bound

import QtQuick

CollectionBrowser {
    id: root

    required property var libraryModel

    title: "专辑"
    emptyTitle: "还没有专辑"
    emptySubtitle: "添加音乐后，专辑将显示在这里"
    collectionModel: root.libraryModel.album_model
    detailModel: root.libraryModel.album_detail
    selectedName: root.libraryModel.selected_album
    selectedSubtitle: root.libraryModel.selected_album_subtitle
    selectedCover: root.libraryModel.selected_album_cover
    onCollectionOpened: function(collectionId) { root.libraryModel.open_album(collectionId) }
    onCollectionClosed: root.libraryModel.close_album()
    onTrackActivated: function(trackId) { root.libraryModel.play_album_track(trackId) }
}
