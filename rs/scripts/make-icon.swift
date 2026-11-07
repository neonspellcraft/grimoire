// Draws the cross-platform Grimoire's app icon: a little curved CRT screen with a phosphor sigil and a prompt.
// Usage (from rs/): swift scripts/make-icon.swift
//
// Writes:
//   packaging/icons/Grimoire.icns          macOS (the icon-grid rounded square, with margin and shadow)
//   packaging/icons/icon-1024.png          macOS artwork, for previews
//   packaging/icons/windows-1024.png       Windows artwork (fills more of the square; scripts turn it into .ico)
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

/// Draws the icon in a 1024×1024 space (origin bottom-left). `body` is the outer rounded square.
func drawIcon(_ c: CGContext, body: CGRect, corner: CGFloat, shadow: Bool) {
    let s = body.width / 824   // design units are for the 824 pt macOS body
    func p(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: body.minX + x * s, y: body.minY + y * s) }

    // Bezel: near-black violet plastic with a faint top highlight.
    let bodyPath = CGPath(roundedRect: body, cornerWidth: corner, cornerHeight: corner, transform: nil)
    if shadow {
        c.saveGState()
        c.setShadow(offset: CGSize(width: 0, height: -12), blur: 28, color: CGColor(gray: 0, alpha: 0.5))
        c.addPath(bodyPath); c.setFillColor(color(0x0A0810)); c.fillPath()
        c.restoreGState()
    }
    c.saveGState()
    c.addPath(bodyPath); c.clip()
    let bezel = CGGradient(colorsSpace: space, colors: [color(0x241B33), color(0x0B0912), color(0x07060B)] as CFArray,
                           locations: [0, 0.55, 1])!
    c.drawLinearGradient(bezel, start: CGPoint(x: body.midX, y: body.maxY), end: CGPoint(x: body.midX, y: body.minY), options: [])
    c.restoreGState()
    c.addPath(bodyPath)
    c.setStrokeColor(CGColor(gray: 1, alpha: 0.10)); c.setLineWidth(4 * s); c.strokePath()

    // The screen: a slightly bulging rounded rectangle, dark with a green cast, glowing at the center.
    let screen = CGRect(origin: p(62, 92), size: CGSize(width: 700 * s, height: 660 * s))
    let screenPath = CGPath(roundedRect: screen, cornerWidth: 118 * s, cornerHeight: 118 * s, transform: nil)
    c.addPath(screenPath); c.setFillColor(color(0x020304)); c.fillPath()
    c.saveGState()
    c.addPath(screenPath); c.clip()
    let center = CGPoint(x: screen.midX, y: screen.midY + 40 * s)
    let phosphor = CGGradient(colorsSpace: space, colors: [color(0x123526), color(0x071510), color(0x020304)] as CFArray,
                              locations: [0, 0.45, 1])!
    c.drawRadialGradient(phosphor, startCenter: center, startRadius: 0, endCenter: center, endRadius: 430 * s, options: [])

    // A soft violet bloom, then a ring of rune ticks behind the sigil.
    let ring = CGPoint(x: screen.midX, y: screen.midY + 62 * s)
    let bloom = CGGradient(colorsSpace: space, colors: [color(violet, 0.42), color(violet, 0.10), color(violet, 0)] as CFArray,
                           locations: [0, 0.45, 1])!
    c.drawRadialGradient(bloom, startCenter: ring, startRadius: 0, endCenter: ring, endRadius: 300 * s, options: [])
    c.saveGState()
    c.setShadow(offset: .zero, blur: 10 * s, color: color(violet, 0.8))
    c.setStrokeColor(color(violet, 0.55)); c.setLineWidth(3 * s)
    c.addEllipse(in: CGRect(x: ring.x - 214 * s, y: ring.y - 214 * s, width: 428 * s, height: 428 * s)); c.strokePath()
    // Rune-like marks around the ring: short strokes, some with a branch, at irregular-looking but fixed angles.
    c.setLineWidth(5 * s); c.setLineCap(.round)
    for i in 0..<24 {
        let angle = CGFloat(i) / 24 * 2 * .pi + 0.13
        let (dx, dy) = (cos(angle), sin(angle))
        let (a, b) = (226 * s, 248 * s)
        c.move(to: CGPoint(x: ring.x + dx * a, y: ring.y + dy * a))
        c.addLine(to: CGPoint(x: ring.x + dx * b, y: ring.y + dy * b))
        if i % 3 != 1 {
            let mid = 237 * s, side: CGFloat = i % 2 == 0 ? 1 : -1
            c.move(to: CGPoint(x: ring.x + dx * mid, y: ring.y + dy * mid))
            c.addLine(to: CGPoint(x: ring.x + dx * (mid + 12 * s) - dy * 10 * s * side,
                                  y: ring.y + dy * (mid + 12 * s) + dx * 10 * s * side))
        }
    }
    c.setStrokeColor(color(violet, 0.6)); c.strokePath()
    c.restoreGState()

    // The sigil: eight tapering rays of violet light with a gap around a bright four-point star.
    func ray(angle: CGFloat, from inner: CGFloat, to outer: CGFloat, width: CGFloat) {
        let (dx, dy) = (cos(angle), sin(angle)), (nx, ny) = (-sin(angle), cos(angle))
        let path = CGMutablePath()
        path.move(to: CGPoint(x: ring.x + dx * inner + nx * width, y: ring.y + dy * inner + ny * width))
        path.addLine(to: CGPoint(x: ring.x + dx * outer, y: ring.y + dy * outer))
        path.addLine(to: CGPoint(x: ring.x + dx * inner - nx * width, y: ring.y + dy * inner - ny * width))
        path.closeSubpath()
        c.addPath(path)
    }
    for pass in 0..<2 {
        c.saveGState()
        if pass == 0 { c.setShadow(offset: .zero, blur: 40 * s, color: color(violet, 0.95)) }
        c.setFillColor(pass == 0 ? color(violet) : color(0xE4D4FF))
        for i in 0..<8 {
            let angle = CGFloat(i) * .pi / 4
            let straight = i % 2 == 0
            ray(angle: angle, from: 70 * s, to: (straight ? 206 : 128) * s, width: (straight ? 13 : 8) * s * (pass == 0 ? 1 : 0.4))
        }
        c.fillPath()
        c.restoreGState()
    }
    // Center star (✦): concave four-pointed, white-green core with a strong green glow.
    let star = CGMutablePath()
    let (outer, inner) = (62 * s, 13 * s)
    for i in 0..<8 {
        let angle = CGFloat(i) * .pi / 4 + .pi / 2
        let r = i % 2 == 0 ? outer : inner
        let point = CGPoint(x: ring.x + cos(angle) * r, y: ring.y + sin(angle) * r)
        i == 0 ? star.move(to: point) : star.addLine(to: point)
    }
    star.closeSubpath()
    for (blur, fill) in [(70 * s, color(green)), (22 * s, color(green)), (0, color(0xF2FFF7))] as [(CGFloat, CGColor)] {
        c.saveGState()
        if blur > 0 { c.setShadow(offset: .zero, blur: blur, color: color(green, 1)) }
        c.addPath(star); c.setFillColor(fill); c.fillPath()
        c.restoreGState()
    }

    // The prompt: a violet chevron and a green block cursor, like the main window's "> invoke".
    let base = p(150, 140)
    c.saveGState()
    c.setShadow(offset: .zero, blur: 18 * s, color: color(violet, 0.9))
    c.setStrokeColor(color(violet)); c.setLineWidth(15 * s); c.setLineCap(.round); c.setLineJoin(.round)
    c.move(to: CGPoint(x: base.x, y: base.y + 62 * s))
    c.addLine(to: CGPoint(x: base.x + 38 * s, y: base.y + 31 * s))
    c.addLine(to: CGPoint(x: base.x, y: base.y))
    c.strokePath()
    c.restoreGState()
    c.saveGState()
    c.setShadow(offset: .zero, blur: 18 * s, color: color(green, 0.9))
    c.setFillColor(color(green, 0.55))
    for i in 0..<3 { c.fill(CGRect(x: base.x + (86 + CGFloat(i) * 46) * s, y: base.y + 4 * s, width: 30 * s, height: 8 * s)) }
    c.setFillColor(color(green))
    c.fill(CGRect(x: base.x + 228 * s, y: base.y - 4 * s, width: 34 * s, height: 70 * s))
    c.restoreGState()

    // Scanlines, a vignette, and a glass highlight.
    c.setFillColor(CGColor(gray: 0, alpha: 0.13))
    var y = screen.minY
    while y < screen.maxY { c.fill(CGRect(x: screen.minX, y: y, width: screen.width, height: 3 * s)); y += 9 * s }
    let vignette = CGGradient(colorsSpace: space, colors: [CGColor(gray: 0, alpha: 0), CGColor(gray: 0, alpha: 0.55)] as CFArray,
                              locations: [0.55, 1])!
    c.drawRadialGradient(vignette, startCenter: CGPoint(x: screen.midX, y: screen.midY), startRadius: 0,
                         endCenter: CGPoint(x: screen.midX, y: screen.midY), endRadius: 470 * s, options: [])
    let glass = CGGradient(colorsSpace: space, colors: [CGColor(gray: 1, alpha: 0.10), CGColor(gray: 1, alpha: 0)] as CFArray,
                           locations: [0, 1])!
    c.drawLinearGradient(glass, start: CGPoint(x: screen.minX, y: screen.maxY), end: CGPoint(x: screen.midX, y: screen.midY), options: [])
    c.restoreGState()
    c.addPath(screenPath)
    c.setStrokeColor(CGColor(gray: 0, alpha: 0.85)); c.setLineWidth(10 * s); c.strokePath()
    c.addPath(screenPath)
    c.setStrokeColor(color(violet, 0.18)); c.setLineWidth(2.5 * s); c.strokePath()

    // A little green power LED on the bezel's chin.
    let led = p(700, 46)
    c.saveGState()
    c.setShadow(offset: .zero, blur: 16 * s, color: color(green, 1))
    c.setFillColor(color(green))
    c.fillEllipse(in: CGRect(x: led.x - 9 * s, y: led.y - 9 * s, width: 18 * s, height: 18 * s))
    c.restoreGState()
}

func render(size: Int, body: CGRect, corner: CGFloat, shadow: Bool) -> CGImage {
    let c = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0, space: space,
                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    c.scaleBy(x: CGFloat(size) / 1024, y: CGFloat(size) / 1024)
    drawIcon(c, body: body, corner: corner, shadow: shadow)
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

save(render(size: 1024, body: mac.body, corner: mac.corner, shadow: true), "packaging/icons/icon-1024.png")
save(render(size: 1024, body: windows.body, corner: windows.corner, shadow: false), "packaging/icons/windows-1024.png")
save(render(size: 256, body: windows.body, corner: windows.corner, shadow: false), "crates/grimoire-app/assets/icon.png")

// macOS iconset → .icns
let iconset = "packaging/icons/Grimoire.iconset"
try? FileManager.default.removeItem(atPath: iconset)
for (points, scales) in [(16, [1, 2]), (32, [1, 2]), (128, [1, 2]), (256, [1, 2]), (512, [1, 2])] {
    for scale in scales {
        let suffix = scale == 1 ? "" : "@2x"
        save(render(size: points * scale, body: mac.body, corner: mac.corner, shadow: true),
             "\(iconset)/icon_\(points)x\(points)\(suffix).png")
    }
}
let iconutil = Process()
iconutil.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
iconutil.arguments = ["-c", "icns", iconset, "-o", "packaging/icons/Grimoire.icns"]
try! iconutil.run()
iconutil.waitUntilExit()
try? FileManager.default.removeItem(atPath: iconset)
print("Wrote packaging/icons/Grimoire.icns, icon-1024.png, windows-1024.png and assets/icon.png")
