import AppKit
import GrimoireCore
import SwiftUI

struct PlaylistView: View {
    let player: PlayerController
    let windows: WindowManager
    @State private var selection = Set<Track.ID>()

    var body: some View {
        VStack(spacing: 6) {
            TitleBar(title: "PLAYLIST", onClose: windows.togglePlaylist, onDragEnded: windows.snapPlaylist)
            List(selection: $selection) {
                ForEach(Array(player.playlist.tracks.enumerated()), id: \.element.id) { index, track in
                    PlaylistRow(number: index + 1, track: track, isCurrent: track.id == player.playlist.currentID)
                        .listRowBackground(Color.clear)
                }
                .onMove { player.move(fromOffsets: $0, toOffset: $1) }
                .dropDestination(for: URL.self) { urls, index in
                    player.add(urls, at: index)
                }
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .environment(\.defaultMinListRowHeight, 18)
            .contextMenu(forSelectionType: Track.ID.self) { ids in
                Button("Play") { if let id = ids.first { player.playTrack(id) } }
                Button("Show in Finder") {
                    let urls = player.playlist.tracks.filter { ids.contains($0.id) }.map(\.url)
                    NSWorkspace.shared.activateFileViewerSelecting(urls)
                }
                Divider()
                Button("Remove") { remove(ids) }
            } primaryAction: { ids in
                if let id = ids.first { player.playTrack(id) }
            }
            .onDeleteCommand { remove(selection) }
            .padding(2)
            .lcdPanel()

            HStack(spacing: 3) {
                barButton("ADD", help: "Add files or folders") { player.openFiles() }
                barButton("REM", help: "Remove selected") { remove(selection) }
                barButton("CLR", help: "Clear playlist") { player.clear() }
                barButton("LOAD", help: "Open playlist…") { player.loadPlaylist() }
                barButton("SAVE", help: "Save playlist…") { player.savePlaylist() }
                Spacer(minLength: 6)
                Text(summary)
                    .font(Theme.lcdFont(9))
                    .foregroundStyle(Theme.lcdDim)
                    .lineLimit(1)
                ResizeGrip().frame(width: 12, height: 12)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .frame(minWidth: 320, minHeight: 160)
        .background(WindowChrome(onDragEnded: windows.snapPlaylist))
        .dropDestination(for: URL.self) { urls, _ in
            player.add(urls)
            return true
        }
    }

    private var summary: String {
        let count = player.playlist.tracks.count
        guard count > 0 else { return "drop files here" }
        return "\(count) track\(count == 1 ? "" : "s") · \(TimeFormat.clock(player.playlist.totalDuration))"
    }

    private func remove(_ ids: Set<Track.ID>) {
        player.remove(ids)
        selection.subtract(ids)
    }

    private func barButton(_ title: String, help: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(Theme.labelFont(8))
                .tracking(1)
                .padding(.horizontal, 6)
                .frame(height: 18)
        }
        .buttonStyle(RetroButtonStyle())
        .help(help)
    }
}

private struct PlaylistRow: View {
    let number: Int
    let track: Track
    let isCurrent: Bool

    var body: some View {
        HStack(spacing: 6) {
            Text("\(number).")
                .foregroundStyle(isCurrent ? Theme.accent : Theme.lcdDim)
                .frame(width: 34, alignment: .trailing)
            Text(track.displayTitle)
                .foregroundStyle(isCurrent ? Color.white : Theme.lcdText)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 8)
            Text(track.duration.map(TimeFormat.clock) ?? "")
                .foregroundStyle(isCurrent ? Color.white : Theme.lcdDim)
        }
        .font(.system(size: 11, weight: isCurrent ? .bold : .regular, design: .monospaced))
        .contentShape(Rectangle())
    }
}
