// Draws the cross-platform Grimoire's app icon: one bold glowing sigil on a near-black rounded square.
// Usage (from rs/):
//   swift scripts/make-icon.swift                 writes the icons below with the chosen sigil
//   swift scripts/make-icon.swift --preview DIR   writes one PNG per candidate sigil to DIR, for choosing
//
// Writes:
//   packaging/icons/Grimoire.icns          macOS (the icon-grid rounded square, with margin and shadow)
//   packaging/icons/icon-1024.png          macOS artwork, for previews
//   packaging/icons/windows-1024.png       Windows artwork (fills more of the square; turned into .ico)
//   crates/grimoire-app/assets/icon.png    256 px window icon (Windows title bar and taskbar)
import AppKit
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

let space = CGColorSpace(name: CGColorSpace.sRGB)!
func color(_ hex: UInt32, _ alpha: CGFloat = 1) -> CGColor {
    CGColor(srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255, alpha: alpha)
}
// The app's palette (src/ui/theme.rs).
let green = 0x73FFAD as UInt32
let violet = 0xB380FF as UInt32

enum Sigil: String, CaseIterable {
    /// A stave with branches up and down (ᛉ over ᛣ), crossed at its heart: a bind rune.
    case bindRune
    /// A bold four-pointed star, the ✦ the app uses for the playing track.
    case star
    /// An eye-shaped vesica pierced by a stave, with a bright pupil.
    case eye
}
let chosen = Sigil.eye

/// Strokes the sigil in layers: a wide violet bloom, the violet body, then a hot white-green core.
func drawSigil(_ sigil: Sigil, _ c: CGContext, center o: CGPoint, scale s: CGFloat) {
    func pt(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: o.x + x * s, y: o.y + y * s) }
    let path = CGMutablePath()
    var fill: CGPath?
    switch sigil {
    case .bindRune:
        path.move(to: pt(0, -300)); path.addLine(to: pt(0, 300))                                      // the stave
        path.move(to: pt(-190, 300)); path.addLine(to: pt(0, 120)); path.addLine(to: pt(190, 300))     // ᛉ
        path.move(to: pt(-190, -300)); path.addLine(to: pt(0, -120)); path.addLine(to: pt(190, -300))  // ᛣ
        path.addEllipse(in: CGRect(x: o.x - 70 * s, y: o.y - 70 * s, width: 140 * s, height: 140 * s))
    case .star:
        let star = CGMutablePath()
        for i in 0..<8 {
            let angle = CGFloat(i) * .pi / 4 + .pi / 2
            let r = CGFloat(i % 2 == 0 ? 330 : 62) * s
            let p = CGPoint(x: o.x + cos(angle) * r, y: o.y + sin(angle) * r)
            if i == 0 { star.move(to: p) } else { star.addLine(to: p) }
        }
        star.closeSubpath()
        fill = star
    case .eye:
        path.move(to: pt(-320, 0))
        path.addQuadCurve(to: pt(320, 0), control: pt(0, 300))
        path.addQuadCurve(to: pt(-320, 0), control: pt(0, -300))
        path.move(to: pt(0, -330)); path.addLine(to: pt(0, -150))
        path.move(to: pt(0, 150)); path.addLine(to: pt(0, 330))
        fill = CGPath(ellipseIn: CGRect(x: o.x - 62 * s, y: o.y - 62 * s, width: 124 * s, height: 124 * s), transform: nil)
    }
    c.setLineCap(.round); c.setLineJoin(.round)
    let layers: [(blur: CGFloat, glow: UInt32, stroke: UInt32, width: CGFloat)] = [
        (90, violet, violet, 62), (34, violet, violet, 54), (16, green, 0xE6FFF0, 20),
    ]
    for layer in layers {
        c.saveGState()
        c.setShadow(offset: .zero, blur: layer.blur * s, color: color(layer.glow, 1))
        c.setStrokeColor(color(layer.stroke)); c.setLineWidth(layer.width * s)
        if !path.isEmpty { c.addPath(path); c.strokePath() }
        if let fill {
            c.setFillColor(color(layer.stroke))
            c.addPath(fill); c.fillPath()
        }
        c.restoreGState()
    }
}

/// Draws the icon in a 1024×1024 space (origin bottom-left). `body` is the outer rounded square.
func drawIcon(_ c: CGContext, sigil: Sigil, body: CGRect, corner: CGFloat, shadow: Bool) {
    let s = body.width / 824   // design units are for the 824 pt macOS body
    let bodyPath = CGPath(roundedRect: body, cornerWidth: corner, cornerHeight: corner, transform: nil)
    if shadow {
        c.saveGState()
        c.setShadow(offset: CGSize(width: 0, height: -12), blur: 28, color: CGColor(gray: 0, alpha: 0.5))
        c.addPath(bodyPath); c.setFillColor(color(0x07060B)); c.fillPath()
        c.restoreGState()
    }
    c.saveGState()
    c.addPath(bodyPath); c.clip()
    // Near-black violet, a touch lighter behind the sigil.
    let center = CGPoint(x: body.midX, y: body.midY)
    let ground = CGGradient(colorsSpace: space, colors: [color(0x1C1530), color(0x0B0913), color(0x050408)] as CFArray,
                            locations: [0, 0.6, 1])!
    c.drawRadialGradient(ground, startCenter: center, startRadius: 0, endCenter: center, endRadius: 560 * s, options: [])
    drawSigil(sigil, c, center: center, scale: s * 0.92)
    // Faint scanlines.
    c.setFillColor(CGColor(gray: 0, alpha: 0.16))
    var y = body.minY
    while y < body.maxY { c.fill(CGRect(x: body.minX, y: y, width: body.width, height: 3 * s)); y += 9 * s }
    c.restoreGState()
    c.addPath(bodyPath)
    c.setStrokeColor(color(violet, 0.22)); c.setLineWidth(4 * s); c.strokePath()
}

func render(_ sigil: Sigil, size: Int, body: CGRect, corner: CGFloat, shadow: Bool) -> CGImage {
    let c = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0, space: space,
                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    c.interpolationQuality = .high
    c.scaleBy(x: CGFloat(size) / 1024, y: CGFloat(size) / 1024)
    drawIcon(c, sigil: sigil, body: body, corner: corner, shadow: shadow)
    return c.makeImage()!
}

func save(_ image: CGImage, _ path: String) {
    let url = URL(fileURLWithPath: path)
    try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)!
    CGImageDestinationAddImage(destination, image, nil)
    precondition(CGImageDestinationFinalize(destination), "couldn't write \(path)")
}

let mac = (body: CGRect(x: 100, y: 100, width: 824, height: 824), corner: CGFloat(186))
let windows = (body: CGRect(x: 24, y: 24, width: 976, height: 976), corner: CGFloat(170))

let arguments = CommandLine.arguments
if let flag = arguments.firstIndex(of: "--preview"), flag + 1 < arguments.count {
    // A sheet per candidate: the full icon, then how it reads at 128, 64 (Dock) and 32 (Finder list) px.
    let directory = arguments[flag + 1]
    for sigil in Sigil.allCases {
        let sheet = CGContext(data: nil, width: 1024 + 220, height: 1024, bitsPerComponent: 8, bytesPerRow: 0, space: space,
                              bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        sheet.setFillColor(color(0x2A2A2E)); sheet.fill(CGRect(x: 0, y: 0, width: 1244, height: 1024))
        sheet.draw(render(sigil, size: 1024, body: mac.body, corner: mac.corner, shadow: true),
                   in: CGRect(x: 0, y: 0, width: 1024, height: 1024))
        var y: CGFloat = 760
        for size in [128, 64, 32] {
            sheet.draw(render(sigil, size: size, body: mac.body, corner: mac.corner, shadow: true),
                       in: CGRect(x: 1060, y: y, width: CGFloat(size), height: CGFloat(size)))
            y -= CGFloat(size) + 60
        }
        save(sheet.makeImage()!, "\(directory)/sigil-\(sigil.rawValue).png")
    }
    print("Wrote \(Sigil.allCases.count) candidates to \(directory)")
    exit(0)
}

save(render(chosen, size: 1024, body: mac.body, corner: mac.corner, shadow: true), "packaging/icons/icon-1024.png")
save(render(chosen, size: 1024, body: windows.body, corner: windows.corner, shadow: false), "packaging/icons/windows-1024.png")
save(render(chosen, size: 256, body: windows.body, corner: windows.corner, shadow: false), "crates/grimoire-app/assets/icon.png")

let iconset = "packaging/icons/Grimoire.iconset"
try? FileManager.default.removeItem(atPath: iconset)
for points in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let suffix = scale == 1 ? "" : "@2x"
        save(render(chosen, size: points * scale, body: mac.body, corner: mac.corner, shadow: true),
             "\(iconset)/icon_\(points)x\(points)\(suffix).png")
    }
}
let iconutil = Process()
iconutil.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
iconutil.arguments = ["-c", "icns", iconset, "-o", "packaging/icons/Grimoire.icns"]
try! iconutil.run()
iconutil.waitUntilExit()
try? FileManager.default.removeItem(atPath: iconset)
print("Wrote packaging/icons/Grimoire.icns, icon-1024.png, windows-1024.png and assets/icon.png (\(chosen.rawValue))")
