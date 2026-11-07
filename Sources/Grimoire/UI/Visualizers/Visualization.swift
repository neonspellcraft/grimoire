import AppKit
import SwiftUI

/// A visualizer style. To add one: write a class conforming to this, then add a case to `VisualizationKind`.
///
/// Drawing uses Core Graphics in a dedicated layer-backed view (see `VisualizerView`), so each frame repaints
/// only the visualizer instead of making SwiftUI re-composite the whole window.
@MainActor
protocol Visualization: AnyObject {
    /// Called once per display frame. `window` holds the latest mono samples, or nil when nothing is playing.
    func update(window: UnsafeBufferPointer<Float>?, sampleRate: Double, dt: Double)
    /// Draws into a flipped context (origin at the top left) of the given size.
    func draw(in context: CGContext, size: CGSize)
    /// True when there's nothing left to animate (e.g. bars have fallen to zero), so rendering can pause.
    var isIdle: Bool { get }
}

enum VisualizationKind: String, CaseIterable, Identifiable {
    case spectrum, scope, off

    var id: Self { self }

    var title: String {
        switch self {
        case .spectrum: "Spectrum"
        case .scope: "Oscilloscope"
        case .off: "Off"
        }
    }

    var next: VisualizationKind {
        let all = Self.allCases
        return all[(all.firstIndex(of: self)! + 1) % all.count]
    }

    @MainActor
    func make() -> Visualization? {
        switch self {
        case .spectrum: SpectrumVisualization()
        case .scope: ScopeVisualization()
        case .off: nil
        }
    }
}

extension Color {
    var cgColor: CGColor { NSColor(self).cgColor }
}
