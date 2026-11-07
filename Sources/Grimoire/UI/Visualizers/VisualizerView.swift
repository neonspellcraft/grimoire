import AppKit
import SwiftUI

/// The visualizer area under the LCD time. Click to cycle styles; right-click to pick one.
struct VisualizerView: View {
    let player: PlayerController

    var body: some View {
        Group {
            if player.visualization == .off {
                Sigil()
                    .contentShape(Rectangle())
                    .onTapGesture { player.visualization = player.visualization.next }
                    .contextMenu {
                        ForEach(VisualizationKind.allCases) { kind in
                            Button(kind.title) { player.visualization = kind }
                        }
                    }
            } else {
                VisualizerLayer(
                    kind: player.visualization, playing: player.state == .playing, tap: player.audioTap,
                    select: { player.visualization = $0 })
            }
        }
        .frame(height: 34)
        .help("Click to change visualization")
    }

    /// Shown when the visualizer is off: a faint grid and the sigil, as in v0.1.
    private struct Sigil: View {
        var body: some View {
            ZStack {
                VStack(spacing: 5) {
                    ForEach(0..<5, id: \.self) { _ in
                        Rectangle().fill(Theme.lcdGhost).frame(height: 1)
                    }
                }
                Text("✦  ✦  ✦")
                    .font(.system(size: 9))
                    .foregroundStyle(Theme.lcdText.opacity(0.18))
            }
        }
    }
}

private struct VisualizerLayer: NSViewRepresentable {
    let kind: VisualizationKind
    let playing: Bool
    let tap: AudioTap
    let select: (VisualizationKind) -> Void

    func makeNSView(context: Context) -> VisualizerNSView {
        VisualizerNSView(tap: tap)
    }

    func updateNSView(_ view: VisualizerNSView, context: Context) {
        view.select = select
        view.kind = kind
        view.playing = playing
    }
}

/// Layer-backed view that pulls sample windows from the tap on every display refresh and draws the current
/// `Visualization`.
///
/// The tap fills its buffer in ~100 ms chunks, so this view keeps its own read cursor that advances in real time
/// and trails the newest sample by about one chunk. That gives smooth motion at the display's frame rate.
/// The display link pauses once playback stops and the visualization has settled, so idle CPU is zero.
final class VisualizerNSView: NSView {
    var select: ((VisualizationKind) -> Void)?
    var kind: VisualizationKind = .off {
        didSet {
            guard kind != oldValue else { return }
            visualization = kind.make()
            needsDisplay = true
            updateDisplayLink()
        }
    }
    var playing = false {
        didSet { if playing != oldValue { updateDisplayLink() } }
    }

    private let tap: AudioTap
    private var visualization: Visualization?
    private var displayLink: CADisplayLink?
    private var cursor: Double = 0
    private var lastWritten = 0
    private var lastTimestamp: CFTimeInterval?
    private var samples = [Float](repeating: 0, count: 2048)

    init(tap: AudioTap) {
        self.tap = tap
        super.init(frame: .zero)
        wantsLayer = true
        layerContentsRedrawPolicy = .onSetNeedsDisplay
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError() }

    override var isFlipped: Bool { true }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        displayLink?.invalidate()
        displayLink = nil
        if window != nil {
            let link = displayLink(target: self, selector: #selector(step(_:)))
            // 60 fps is plenty for this look; ProMotion displays would otherwise run it at 120.
            link.preferredFrameRateRange = CAFrameRateRange(minimum: 30, maximum: 60, preferred: 60)
            link.add(to: .main, forMode: .common)
            displayLink = link
            updateDisplayLink()
        }
    }

    private func updateDisplayLink() {
        lastTimestamp = nil
        displayLink?.isPaused = !playing && (visualization?.isIdle ?? true)
    }

    @objc private func step(_ link: CADisplayLink) {
        guard let visualization else {
            link.isPaused = true
            return
        }
        let dt = lastTimestamp.map { min(max(link.timestamp - $0, 0), 0.1) } ?? 0
        lastTimestamp = link.timestamp
        let position = tap.position

        var hasWindow = false
        if playing, position.written >= samples.count {
            if position.written < lastWritten || cursor > Double(position.written + position.chunk) {
                cursor = Double(position.written - position.chunk)   // tap was reset (new track)
            }
            cursor += dt * position.sampleRate
            let newest = Double(position.written)
            cursor = min(max(cursor, newest - 2 * Double(position.chunk)), newest)
            hasWindow = samples.withUnsafeMutableBufferPointer {
                tap.copyWindow(endingAt: Int(cursor), into: $0)
            }
        }
        lastWritten = position.written

        if hasWindow {
            samples.withUnsafeBufferPointer {
                visualization.update(window: $0, sampleRate: position.sampleRate, dt: dt)
            }
        } else {
            visualization.update(window: nil, sampleRate: position.sampleRate, dt: dt)
        }
        needsDisplay = true
        if !playing && visualization.isIdle { link.isPaused = true }
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        visualization?.draw(in: context, size: bounds.size)
    }

    override func mouseUp(with event: NSEvent) {
        select?(kind.next)
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        let menu = NSMenu()
        for option in VisualizationKind.allCases {
            let item = NSMenuItem(title: option.title, action: #selector(choose(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = option.rawValue
            item.state = option == kind ? .on : .off
            menu.addItem(item)
        }
        return menu
    }

    @objc private func choose(_ item: NSMenuItem) {
        if let raw = item.representedObject as? String, let option = VisualizationKind(rawValue: raw) {
            select?(option)
        }
    }
}
