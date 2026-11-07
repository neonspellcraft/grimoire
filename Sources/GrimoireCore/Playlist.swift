import Foundation

public enum RepeatMode: String, Sendable, CaseIterable {
    case off, all, one

    public var next: RepeatMode {
        switch self {
        case .off: .all
        case .all: .one
        case .one: .off
        }
    }
}

public struct Playlist: Sendable {
    public private(set) var tracks: [Track] = []
    public private(set) var currentID: Track.ID?
    public var repeatMode: RepeatMode = .off
    public var shuffle = false {
        didSet { if shuffle != oldValue { rebuildShuffleOrder() } }
    }
    private var shuffleOrder: [Track.ID] = []

    public init(tracks: [Track] = []) {
        self.tracks = tracks
    }

    public var currentIndex: Int? {
        guard let currentID else { return nil }
        return tracks.firstIndex { $0.id == currentID }
    }

    public var current: Track? { currentIndex.map { tracks[$0] } }

    public var totalDuration: TimeInterval { tracks.reduce(0) { $0 + ($1.duration ?? 0) } }

    // MARK: Editing

    public mutating func append(_ new: [Track]) {
        insert(new, at: tracks.count)
    }

    public mutating func insert(_ new: [Track], at index: Int) {
        tracks.insert(contentsOf: new, at: min(max(0, index), tracks.count))
        if shuffle { shuffleOrder += new.map(\.id).shuffled() }
    }

    public mutating func remove(ids: Set<Track.ID>) {
        tracks.removeAll { ids.contains($0.id) }
        shuffleOrder.removeAll { ids.contains($0) }
        if let currentID, ids.contains(currentID) { self.currentID = nil }
    }

    public mutating func removeAll() {
        tracks = []
        shuffleOrder = []
        currentID = nil
    }

    public mutating func move(fromOffsets source: IndexSet, toOffset destination: Int) {
        let moving = source.map { tracks[$0] }
        let target = destination - source.filter { $0 < destination }.count
        for index in source.reversed() { tracks.remove(at: index) }
        tracks.insert(contentsOf: moving, at: target)
    }

    public mutating func update(_ id: Track.ID, _ change: (inout Track) -> Void) {
        guard let index = tracks.firstIndex(where: { $0.id == id }) else { return }
        change(&tracks[index])
    }

    // MARK: Navigation

    /// Makes `id` the current track. In shuffle mode the remaining order is reshuffled from here.
    public mutating func select(_ id: Track.ID?) {
        currentID = id
        if shuffle { rebuildShuffleOrder() }
    }

    /// Moves to and returns the track after the current one, or nil at the end of the list.
    /// `automatic` is true when the current track ended by itself (so repeat-one replays it).
    public mutating func advance(automatic: Bool) -> Track? {
        guard !tracks.isEmpty else { return nil }
        if automatic, repeatMode == .one, let current { return current }
        let order = playOrder
        guard let currentID, let position = order.firstIndex(of: currentID) else { return setCurrent(order.first) }
        if position + 1 < order.count { return setCurrent(order[position + 1]) }
        guard repeatMode != .off else { return nil }
        if shuffle { rebuildShuffleOrder(currentFirst: false) }
        return setCurrent(playOrder.first)
    }

    /// Moves to and returns the track before the current one. At the start it stays put unless repeating.
    public mutating func retreat() -> Track? {
        guard !tracks.isEmpty else { return nil }
        let order = playOrder
        guard let currentID, let position = order.firstIndex(of: currentID) else { return setCurrent(order.first) }
        if position > 0 { return setCurrent(order[position - 1]) }
        return repeatMode == .off ? current : setCurrent(order.last)
    }

    private var playOrder: [Track.ID] { shuffle ? shuffleOrder : tracks.map(\.id) }

    private mutating func setCurrent(_ id: Track.ID?) -> Track? {
        currentID = id
        return current
    }

    private mutating func rebuildShuffleOrder(currentFirst: Bool = true) {
        var ids = tracks.map(\.id).shuffled()
        if currentFirst, let currentID, let index = ids.firstIndex(of: currentID) {
            ids.remove(at: index)
            ids.insert(currentID, at: 0)
        }
        shuffleOrder = ids
    }
}
