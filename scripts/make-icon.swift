// Draws Grimoire's app icon and writes Resources/AppIcon.icns.
// Usage: swift scripts/make-icon.swift   (from the repo root)
//
// The design uses the app's own palette: violet chrome, a dark LCD with green-to-violet spectrum bars,
// and the ✦ sigil glowing above it.
import AppKit
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

let chromeTop = CGColor(srgbRed: 0.20, green: 0.16, blue: 0.29, alpha: 1)
let chromeBottom = CGColor(srgbRed: 0.06, green: 0.05, blue: 0.09, alpha: 1)
let lcd = CGColor(srgbRed: 0.02, green: 0.03, blue: 0.035, alpha: 1)
let green = (r: 0.45, g: 1.0, b: 0.68)
let violet = (r: 0.70, g: 0.50, b: 1.0)
let accent = CGColor(srgbRed: violet.r, green: violet.g, blue: violet.b, alpha: 1)

/// Draws the icon in a 1024×1024 coordinate space (origin bottom-left).
func drawIcon(in context: CGContext) {
    let space = CGColorSpace(name: CGColorSpace.sRGB)!

    // Body: the macOS icon grid's 824pt rounded square, centered.
    let body = CGRect(x: 100, y: 100, width: 824, height: 824)
    let bodyPath = CGPath(roundedRect: body, cornerWidth: 186, cornerHeight: 186, transform: nil)
    context.saveGState()
    context.setShadow(offset: CGSize(width: 0, height: -12), blur: 28, color: CGColor(gray: 0, alpha: 0.45))
    context.addPath(bodyPath)
    context.setFillColor(chromeBottom)
    context.fillPath()
    context.restoreGState()

    context.saveGState()
    context.addPath(bodyPath)
    context.clip()
    let chrome = CGGradient(colorsSpace: space, colors: [chromeTop, chromeBottom] as CFArray, locations: [0, 1])!
    context.drawLinearGradient(chrome, start: CGPoint(x: 512, y: 924), end: CGPoint(x: 512, y: 100), options: [])
    context.restoreGState()
    context.addPath(bodyPath)
    context.setStrokeColor(CGColor(gray: 1, alpha: 0.14))
    context.setLineWidth(6)
    context.strokePath()

    // LCD panel with segmented spectrum bars.
    let panel = CGRect(x: 196, y: 196, width: 632, height: 400)
    let panelPath = CGPath(roundedRect: panel, cornerWidth: 44, cornerHeight: 44, transform: nil)
    context.addPath(panelPath)
    context.setFillColor(lcd)
    context.fillPath()
    context.addPath(panelPath)
    context.setStrokeColor(CGColor(gray: 0, alpha: 0.8))
    context.setLineWidth(6)
    context.strokePath()

    let levels: [CGFloat] = [0.45, 0.72, 0.95, 0.80, 0.58, 0.88, 0.66, 0.40, 0.55, 0.32]
    let inner = panel.insetBy(dx: 46, dy: 44)
    let gap: CGFloat = 14
    let barWidth = (inner.width - gap * CGFloat(levels.count - 1)) / CGFloat(levels.count)
    let segmentHeight: CGFloat = 18, segmentGap: CGFloat = 9
    let segments = Int((inner.height + segmentGap) / (segmentHeight + segmentGap))
    for (bar, level) in levels.enumerated() {
        let x = inner.minX + CGFloat(bar) * (barWidth + gap)
        let lit = Int((level * CGFloat(segments)).rounded())
        for segment in 0..<segments {
            let rect = CGRect(x: x, y: inner.minY + CGFloat(segment) * (segmentHeight + segmentGap), width: barWidth, height: segmentHeight)
            if segment < lit {
                let t = Double(segment) / Double(max(segments - 1, 1))
                context.setFillColor(CGColor(
                    srgbRed: green.r + (violet.r - green.r) * t,
                    green: green.g + (violet.g - green.g) * t,
                    blue: green.b + (violet.b - green.b) * t, alpha: 1))
            } else if segment == lit {
                context.setFillColor(accent)
            } else {
                context.setFillColor(CGColor(srgbRed: green.r, green: green.g, blue: green.b, alpha: 0.08))
            }
            context.fill(rect)
        }
    }

    // The ✦ sigil: a four-pointed star with a soft glow.
    let center = CGPoint(x: 512, y: 760)
    let star = CGMutablePath()
    let outer: CGFloat = 112, waist: CGFloat = 26
    for i in 0..<8 {
        let angle = CGFloat(i) * .pi / 4 + .pi / 2
        let radius = i.isMultiple(of: 2) ? outer : waist
        let point = CGPoint(x: center.x + cos(angle) * radius, y: center.y + sin(angle) * radius)
        i == 0 ? star.move(to: point) : star.addLine(to: point)
    }
    star.closeSubpath()
    context.saveGState()
    context.setShadow(offset: .zero, blur: 40, color: CGColor(srgbRed: violet.r, green: violet.g, blue: violet.b, alpha: 0.9))
    context.addPath(star)
    context.setFillColor(accent)
    context.fillPath()
    context.restoreGState()
    context.addPath(star)
    context.setFillColor(CGColor(srgbRed: 0.92, green: 0.86, blue: 1, alpha: 1))
    context.fillPath()
}

func render(size: Int) -> CGImage {
    let context = CGContext(
        data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0,
        space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    context.scaleBy(x: CGFloat(size) / 1024, y: CGFloat(size) / 1024)
    drawIcon(in: context)
    return context.makeImage()!
}

func writePNG(_ image: CGImage, to url: URL) {
    let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)!
    CGImageDestinationAddImage(destination, image, nil)
    CGImageDestinationFinalize(destination)
}

let root = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
let iconset = FileManager.default.temporaryDirectory.appendingPathComponent("AppIcon.iconset")
try? FileManager.default.removeItem(at: iconset)
try! FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
for base in [16, 32, 128, 256, 512] {
    writePNG(render(size: base), to: iconset.appendingPathComponent("icon_\(base)x\(base).png"))
    writePNG(render(size: base * 2), to: iconset.appendingPathComponent("icon_\(base)x\(base)@2x.png"))
}
let output = root.appendingPathComponent("Resources/AppIcon.icns")
let iconutil = Process()
iconutil.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
iconutil.arguments = ["-c", "icns", iconset.path, "-o", output.path]
try! iconutil.run()
iconutil.waitUntilExit()
writePNG(render(size: 512), to: root.appendingPathComponent("Resources/AppIcon-preview.png"))
print(iconutil.terminationStatus == 0 ? "Wrote \(output.path)" : "iconutil failed")
