import AppKit
import SwiftUI

/// Title strip with the sigil, the window name between engraved lines, and minimize/close buttons.
struct TitleBar: View {
    let title: String
    var onMinimize: (() -> Void)?
    let onClose: () -> Void
    var onDragEnded: (() -> Void)?

    var body: some View {
        HStack(spacing: 6) {
            Text("✦").font(.system(size: 10)).foregroundStyle(Theme.accent)
            Engraving()
            Text(title)
                .font(Theme.labelFont(9))
                .tracking(3)
                .foregroundStyle(Theme.text)
                .fixedSize()
            Engraving()
            if let onMinimize {
                TitleButton(symbol: "minus", action: onMinimize)
            }
            TitleButton(symbol: "xmark", action: onClose)
        }
        .frame(height: 14)
        .background(WindowDragArea(onDragEnded: onDragEnded))
    }

    private struct Engraving: View {
        var body: some View {
            VStack(spacing: 2) {
                ForEach(0..<3, id: \.self) { _ in
                    Rectangle().fill(Theme.accent.opacity(0.35)).frame(height: 1)
                }
            }
            .allowsHitTesting(false)
        }
    }

    private struct TitleButton: View {
        let symbol: String
        let action: () -> Void

        var body: some View {
            Button(action: action) {
                Image(systemName: symbol)
                    .font(.system(size: 7, weight: .black))
                    .frame(width: 13, height: 11)
            }
            .buttonStyle(RetroButtonStyle(cornerRadius: 2))
        }
    }
}

struct RetroButtonStyle: ButtonStyle {
    var isOn = false
    var cornerRadius: CGFloat = 3

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .foregroundStyle(isOn ? Theme.accent : Theme.text)
            .background(
                RoundedRectangle(cornerRadius: cornerRadius).fill(
                    LinearGradient(
                        colors: configuration.isPressed
                            ? [Theme.chromeBottom, Theme.chromeTop]
                            : [Theme.chromeTop.opacity(1.6), Theme.chromeBottom],
                        startPoint: .top, endPoint: .bottom))
            )
            .bevel(inset: configuration.isPressed, cornerRadius: cornerRadius)
            .contentShape(Rectangle())
    }
}

/// Transport button with an SF Symbol.
struct TransportButton: View {
    let symbol: String
    let help: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 12, weight: .bold))
                .frame(width: 38, height: 26)
        }
        .buttonStyle(RetroButtonStyle())
        .help(help)
    }
}

/// Small labelled toggle with an LED that lights up when on.
struct LEDToggle: View {
    let label: String
    let isOn: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                Circle()
                    .fill(isOn ? Theme.lcdText : Theme.lcdGhost)
                    .frame(width: 5, height: 5)
                    .shadow(color: isOn ? Theme.lcdText : .clear, radius: 3)
                Text(label).font(Theme.labelFont(8)).tracking(1)
            }
            .padding(.horizontal, 6)
            .frame(height: 18)
        }
        .buttonStyle(RetroButtonStyle(isOn: isOn))
    }
}

/// Horizontal 0...1 slider drawn in the Grimoire style.
/// With `live` the value is reported while dragging (volume); otherwise only on release (seek).
struct RetroSlider: View {
    let value: Double
    var live = true
    var enabled = true
    let onChange: (Double) -> Void

    @State private var dragValue: Double?
    private let thumbWidth: CGFloat = 16

    var body: some View {
        GeometryReader { geometry in
            let shown = min(max(dragValue ?? value, 0), 1)
            let travel = max(geometry.size.width - thumbWidth, 1)
            let x = travel * shown
            ZStack(alignment: .leading) {
                RoundedRectangle(cornerRadius: 2)
                    .fill(Theme.lcd)
                    .frame(height: 6)
                    .bevel(inset: true, cornerRadius: 2)
                RoundedRectangle(cornerRadius: 2)
                    .fill(Theme.accent.opacity(enabled ? 0.55 : 0))
                    .frame(width: x + thumbWidth / 2, height: 4)
                    .padding(.leading, 1)
                RoundedRectangle(cornerRadius: 2)
                    .fill(LinearGradient(colors: [Color(white: 0.75), Color(white: 0.4)], startPoint: .top, endPoint: .bottom))
                    .frame(width: thumbWidth, height: 12)
                    .bevel(cornerRadius: 2)
                    .offset(x: x)
                    .opacity(enabled ? 1 : 0.3)
            }
            .frame(maxHeight: .infinity)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { gesture in
                        guard enabled else { return }
                        let newValue = min(max((gesture.location.x - thumbWidth / 2) / travel, 0), 1)
                        dragValue = newValue
                        if live { onChange(newValue) }
                    }
                    .onEnded { _ in
                        if let dragValue, !live { onChange(dragValue) }
                        dragValue = nil
                    }
            )
        }
        .frame(height: 14)
    }
}

/// Lets a borderless window be dragged from this area. Reports when a drag finishes (used for docking).
struct WindowDragArea: NSViewRepresentable {
    var onDragEnded: (() -> Void)?

    func makeNSView(context: Context) -> DragView {
        DragView()
    }

    func updateNSView(_ view: DragView, context: Context) {
        view.onDragEnded = onDragEnded
    }

    final class DragView: NSView {
        var onDragEnded: (() -> Void)?

        override func mouseDown(with event: NSEvent) {
            window?.performDrag(with: event)
            onDragEnded?()
        }
    }
}

/// Bottom-right grip that resizes its (borderless) window.
struct ResizeGrip: NSViewRepresentable {
    func makeNSView(context: Context) -> GripView { GripView() }
    func updateNSView(_ view: GripView, context: Context) {}

    final class GripView: NSView {
        private var startMouse: NSPoint = .zero
        private var startFrame: NSRect = .zero

        override func mouseDown(with event: NSEvent) {
            startMouse = NSEvent.mouseLocation
            startFrame = window?.frame ?? .zero
        }

        override func mouseDragged(with event: NSEvent) {
            guard let window else { return }
            let mouse = NSEvent.mouseLocation
            var frame = startFrame
            frame.size.width = max(window.minSize.width, startFrame.width + (mouse.x - startMouse.x))
            frame.size.height = max(window.minSize.height, startFrame.height - (mouse.y - startMouse.y))
            frame.origin.y = startFrame.maxY - frame.height
            window.setFrame(frame, display: true)
        }

        override func resetCursorRects() {
            addCursorRect(bounds, cursor: .crosshair)
        }

        override func draw(_ dirtyRect: NSRect) {
            NSColor.white.withAlphaComponent(0.3).setStroke()
            for offset in stride(from: CGFloat(3), through: 11, by: 4) {
                let path = NSBezierPath()
                path.move(to: NSPoint(x: bounds.maxX - offset, y: bounds.minY + 1))
                path.line(to: NSPoint(x: bounds.maxX - 1, y: bounds.minY + offset))
                path.stroke()
            }
        }
    }
}
