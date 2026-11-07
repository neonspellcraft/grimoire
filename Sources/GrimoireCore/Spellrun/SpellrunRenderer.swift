import Foundation

/// Draws a `SpellrunGame` into a `PixelCanvas`: dithered sky, sun or moon, stars and clouds, two parallax
/// backdrop layers, tiles, entities, particles and a tiny HUD.
public final class SpellrunRenderer {
    public init() {}

    private var width = 0, height = 0
    private var cameraX = 0, cameraY = 0
    private var shakeX = 0, shakeY = 0

    public func render(_ game: SpellrunGame, into canvas: PixelCanvas, levels: BandLevels, showMeters: Bool) {
        width = canvas.width
        height = canvas.height
        let time = game.time
        shakeX = Int((sin(time * 83) * game.shake * 2.5).rounded())
        shakeY = Int((cos(time * 71) * game.shake * 2).rounded())
        cameraX = Int(floor(game.cameraX)) - shakeX
        cameraY = Int(floor(game.cameraY)) + shakeY

        drawBackdrop(game, canvas, high: levels.high)
        drawTiles(game, canvas, low: levels.low)
        drawEntities(game, canvas, high: levels.high)
        drawHUD(game, canvas, levels: levels, showMeters: showMeters)
    }

    // MARK: Coordinates

    @inline(__always) private func screenX(_ worldX: Double) -> Int { Int(floor(worldX)) - cameraX }
    /// Canvas row of the top of something `height` px tall whose bottom sits at `worldY`.
    @inline(__always) private func screenTop(_ worldY: Double, height h: Int) -> Int {
        height - (Int(floor(worldY)) - cameraY) - h
    }

    // MARK: Backdrop

    private func drawBackdrop(_ game: SpellrunGame, _ canvas: PixelCanvas, high: Float) {
        let current = Biome.all[game.biomeIndex]
        let previous = Biome.all[game.previousBiome]
        let fade = Float(game.biomeFade)
        let twinkle = Float(game.time)

        for x in 0..<width {
            let farU = Double(cameraX) * 0.2 + Double(x)
            let nearU = Double(cameraX) * 0.5 + Double(x)
            let farNow = farHeight(current, farU), nearNow = nearHeight(current, nearU)
            let farBefore = fade < 1 ? farHeight(previous, farU) : farNow
            let nearBefore = fade < 1 ? nearHeight(previous, nearU) : nearNow
            for y in 0..<height {
                // During a biome change, each pixel picks old or new by an ordered-dither threshold: a pixel dissolve.
                let usePrevious = fade < 1 && bayer(x, y) > fade
                let b = usePrevious ? previous : current
                let far = usePrevious ? farBefore : farNow
                let near = usePrevious ? nearBefore : nearNow
                let fromBottom = height - y
                let color: UInt32
                if fromBottom < near {
                    color = fromBottom < near - 3 && bayer(x, y) < 0.5 ? b.nearShade : b.near
                } else if fromBottom < far {
                    color = b.far
                } else {
                    let t = Float(y) / Float(height)
                    let band = (t * 7 + bayer(x, y) - 0.5).rounded(.down)
                    var sky = mix(b.skyTop, b.skyBottom, min(max(band, 0), 7) / 7)
                    if b.hasStars, y < height * 2 / 3 {
                        let seed = hash(Int(floor(Double(cameraX) * 0.05)) + x, y)
                        if seed > 0.993 {
                            let sparkle = 0.45 + 0.55 * sin(twinkle * (2 + Float(seed * 9)) + Float(seed * 40))
                            sky = mix(sky, rgb(0xFFFFFF), max(0.2, sparkle * (0.5 + high)))
                        }
                    }
                    color = sky
                }
                canvas.set(x, y, color)
            }
        }

        if let orb = current.orb { drawOrb(canvas, color: orb, moon: current.hasStars) }
        if current.hasClouds { drawClouds(canvas, biome: current) }
    }

    private func farHeight(_ biome: Biome, _ u: Double) -> Int {
        let base = Double(height) * 0.42
        switch biome.backdrop {
        case .mountains:
            return Int(base + 26 * noise(u * 0.011) + 12 * noise(u * 0.035) - 8)
        case .pines:
            let slot = floor(u / 9), local = u - slot * 9
            let treeHeight = 14 + 16 * hash(Int(slot), 7)
            return Int(base - 6 + 8 * noise(u * 0.02) + treeHeight * (1 - abs(local - 4.5) / 4.5))
        case .crystals:
            let slot = floor(u / 13), local = u - slot * 13
            let shard = 10 + 40 * hash(Int(slot), 3)
            let profile = max(0, 1 - abs(local - 6.5) / 3.5)
            return Int(base - 14 + 6 * noise(u * 0.03) + shard * profile)
        case .castle:
            let slot = floor(u / 36), local = u - slot * 36
            let isTower = hash(Int(slot), 11) > 0.45 && local > 8 && local < 26
            let towerHeight = isTower ? 18 + 26 * hash(Int(slot), 5) : 0
            let battlement = isTower && Int(local) % 4 < 2 ? 3.0 : 0
            return Int(base - 10 + towerHeight + battlement)
        }
    }

    private func nearHeight(_ biome: Biome, _ u: Double) -> Int {
        let base = Double(height) * 0.26
        switch biome.backdrop {
        case .mountains:
            return Int(base + 10 * noise(u * 0.02) + 4 * noise(u * 0.07))
        case .pines:
            let slot = floor(u / 7), local = u - slot * 7
            return Int(base + 2 + (10 + 10 * hash(Int(slot), 2)) * (1 - abs(local - 3.5) / 3.5))
        case .crystals:
            let slot = floor(u / 10), local = u - slot * 10
            return Int(base - 4 + (4 + 18 * hash(Int(slot), 9)) * max(0, 1 - abs(local - 5) / 2.5))
        case .castle:
            return Int(base + 4 + (Int(u) % 8 < 4 ? 3 : 0))
        }
    }

    private func drawOrb(_ canvas: PixelCanvas, color: UInt32, moon: Bool) {
        let cx = width * 4 / 5, cy = 24, r = 9
        canvas.glow(cx: cx, cy: cy, radius: r + 7, color, strength: 0.12)
        for dy in -r...r {
            for dx in -r...r where dx * dx + dy * dy <= r * r {
                var c = color
                if moon, hash(dx + 40, dy + 40) > 0.86 { c = mix(color, rgb(0x9A9AB0), 0.5) }
                canvas.set(cx + dx, cy + dy, c)
            }
        }
    }

    private func drawClouds(_ canvas: PixelCanvas, biome: Biome) {
        let cloud = mix(biome.skyBottom, rgb(0xFFFFFF), 0.65)
        let shade = mix(biome.skyBottom, rgb(0xFFFFFF), 0.3)
        let parallax = Double(cameraX) * 0.1
        let spacing = 90.0
        let first = Int(floor(parallax / spacing)) - 1
        for slot in first...(first + Int(Double(width) / spacing) + 2) {
            guard hash(slot, 21) > 0.35 else { continue }
            let cx = Int(Double(slot) * spacing + 40 * hash(slot, 4) - parallax)
            let cy = 14 + Int(30 * hash(slot, 8))
            for (dx, r) in [(0, 6), (7, 8), (15, 5)] {
                for y in -r...r {
                    for x in -r...r where x * x + y * y * 3 <= r * r {
                        canvas.set(cx + dx + x, cy + y, y > r / 3 ? shade : cloud)
                    }
                }
            }
        }
    }

    // MARK: Tiles

    private func drawTiles(_ game: SpellrunGame, _ canvas: PixelCanvas, low: Float) {
        let tile = SpellrunGame.tile
        let firstVisible = Int(floor(Double(cameraX) / Double(tile))) - 1
        let lastVisible = firstVisible + width / tile + 3
        for col in firstVisible...lastVisible {
            guard let c = game.column(col) else { continue }
            let biome = Biome.all[c.biome]
            let x0 = col * tile - cameraX
            for row in 0..<c.ground {
                let top = screenTop(Double(row * tile), height: tile)
                guard top < height, top + tile > 0 else { continue }
                drawGround(canvas, biome, x: x0, y: top, col: col, row: row, isTop: row == c.ground - 1, depth: c.ground - 1 - row)
            }
            if c.platform >= 0 {
                drawPlatform(canvas, biome, x: x0, y: screenTop(Double(c.platform * tile), height: tile), col: col)
            }
            if c.block >= 0 {
                drawBlock(canvas, biome, x: x0, y: screenTop(Double(c.block * tile), height: tile), col: col, low: low)
            }
            if c.ground > 0 {
                let groundTop = screenTop(Double(c.ground * tile), height: 0)
                if c.lantern { drawLantern(canvas, biome, x: x0 + 7, groundY: groundTop, low: low) }
                drawDecor(canvas, biome, kind: c.decor, x: x0, groundY: groundTop, col: col)
            }
        }
    }

    private func drawGround(_ canvas: PixelCanvas, _ b: Biome, x: Int, y: Int, col: Int, row: Int, isTop: Bool, depth: Int) {
        let tile = SpellrunGame.tile
        let fill = depth >= 2 ? mix(b.groundFill, b.groundDark, 0.45) : b.groundFill
        canvas.fillRect(x: x, y: y, width: tile, height: tile, fill)
        for py in 0..<tile {
            for px in 0..<tile where hash(col * tile + px, row * tile + py) > 0.9 {
                canvas.set(x + px, y + py, b.groundDark)
            }
        }
        guard isTop else { return }
        for px in 0..<tile {
            let lip = 3 + (hash(col * tile + px, 99) > 0.6 ? 1 : 0)
            canvas.fillRect(x: x + px, y: y, width: 1, height: lip, b.groundTop)
            canvas.set(x + px, y + lip, b.groundTopShade)
            if hash(col * tile + px, 77) > 0.85 { canvas.set(x + px, y - 1, b.groundTop) }
        }
    }

    private func drawPlatform(_ canvas: PixelCanvas, _ b: Biome, x: Int, y: Int, col: Int) {
        canvas.fillRect(x: x, y: y, width: 16, height: 5, b.platform)
        canvas.fillRect(x: x, y: y + 5, width: 16, height: 1, b.platformShade)
        canvas.fillRect(x: x, y: y, width: 16, height: 1, mix(b.platform, rgb(0xFFFFFF), 0.25))
        canvas.set(x + 3, y + 2, b.platformShade)
        canvas.set(x + 12, y + 2, b.platformShade)
        if col % 2 == 0 { canvas.fillRect(x: x + 15, y: y + 1, width: 1, height: 4, b.platformShade) }
    }

    private func drawBlock(_ canvas: PixelCanvas, _ b: Biome, x: Int, y: Int, col: Int, low: Float) {
        canvas.fillRect(x: x, y: y, width: 16, height: 16, SpellrunArt.outline)
        canvas.fillRect(x: x + 1, y: y + 1, width: 14, height: 14, b.block)
        canvas.fillRect(x: x + 1, y: y + 1, width: 14, height: 1, mix(b.block, rgb(0xFFFFFF), 0.3))
        let glyph = SpellrunArt.runes[abs(col) % SpellrunArt.runes.count]
        let glow = mix(b.block, b.blockGlow, 0.45 + 0.55 * low)
        for (row, line) in glyph.enumerated() {
            for (column, bit) in line.enumerated() where bit == "#" {
                canvas.set(x + 4 + column, y + 4 + row, glow)
            }
        }
        canvas.glow(cx: x + 8, cy: y + 8, radius: 11, b.blockGlow, strength: 0.08 + 0.25 * low)
    }

    private func drawLantern(_ canvas: PixelCanvas, _ b: Biome, x: Int, groundY: Int, low: Float) {
        canvas.fillRect(x: x, y: groundY - 14, width: 2, height: 14, b.platformShade)
        canvas.fillRect(x: x - 1, y: groundY - 18, width: 4, height: 4, SpellrunArt.outline)
        let lamp = mix(mix(b.accent, rgb(0x3A2A20), 0.6), mix(b.accent, rgb(0xFFFFFF), 0.4), low)
        canvas.fillRect(x: x, y: groundY - 17, width: 2, height: 2, lamp)
        canvas.glow(cx: x + 1, cy: groundY - 16, radius: 9, b.accent, strength: 0.05 + 0.5 * low * low)
    }

    private func drawDecor(_ canvas: PixelCanvas, _ b: Biome, kind: UInt8, x: Int, groundY: Int, col: Int) {
        let px = x + 3 + Int(9 * hash(col, 31))
        switch kind {
        case 1:
            canvas.set(px, groundY - 1, b.groundTopShade)
            canvas.set(px + 1, groundY - 2, b.groundTop)
            canvas.set(px + 2, groundY - 1, b.groundTopShade)
        case 2:
            canvas.fillRect(x: px, y: groundY - 3, width: 1, height: 3, b.groundTopShade)
            canvas.set(px, groundY - 4, b.accent)
            canvas.set(px - 1, groundY - 4, mix(b.accent, rgb(0xFFFFFF), 0.4))
        case 3:
            canvas.fillRect(x: px, y: groundY - 4, width: 2, height: 4, b.accent)
            canvas.set(px, groundY - 5, mix(b.accent, rgb(0xFFFFFF), 0.5))
        default:
            break
        }
    }

    // MARK: Entities

    private func drawEntities(_ game: SpellrunGame, _ canvas: PixelCanvas, high: Float) {
        let time = game.time

        for mote in game.motes {
            let bob = sin(time * 3 + mote.phase) * 2
            let frame = SpellrunArt.mote[Int(time * 8 + mote.phase) & 3]
            let x = screenX(mote.x), y = screenTop(mote.y + bob, height: 5)
            canvas.glow(cx: x + 2, cy: y + 2, radius: 5, rgb(0xFFD84D), strength: 0.15 + 0.35 * high)
            canvas.draw(frame, x: x, y: y)
        }

        for enemy in game.enemies {
            let x = screenX(enemy.x)
            switch enemy.kind {
            case .slime:
                let sprite = enemy.dead ? SpellrunArt.slimeSquished : SpellrunArt.slime[Int(time * 4 + enemy.phase) & 1]
                canvas.draw(sprite, x: x, y: screenTop(enemy.y, height: sprite.height), flipped: true)
            case .bat:
                let sprite = SpellrunArt.bat[Int(time * 9 + enemy.phase) & 1]
                if !enemy.dead { canvas.draw(sprite, x: x, y: screenTop(enemy.y, height: sprite.height)) }
            }
        }

        let hero = game.hero
        let sprite: Sprite
        if hero.pounding {
            sprite = SpellrunArt.heroPound
        } else if !hero.grounded {
            sprite = SpellrunArt.heroJump
        } else {
            sprite = SpellrunArt.heroRun[Int(hero.stride / 7) & 1]
        }
        let heroX = screenX(hero.x) - 1, heroY = screenTop(hero.y, height: sprite.height)
        let visible = hero.blink <= 0 || Int(time * 14) % 2 == 0
        if visible { canvas.draw(sprite, x: heroX, y: heroY) }
        // The staff gem glows with the highs.
        canvas.glow(cx: heroX + 10, cy: heroY + 2, radius: 3 + Int(high * 5), rgb(0x74FFAD), strength: 0.2 + 0.6 * high)

        for bolt in game.bolts {
            let x = screenX(bolt.x), y = screenTop(bolt.y, height: 2)
            canvas.glow(cx: x + 1, cy: y + 1, radius: 5, rgb(0x74FFAD), strength: 0.6)
            canvas.fillRect(x: x, y: y, width: 3, height: 2, rgb(0xE6FFF0))
        }

        for particle in game.particles where particle.life > 0 {
            let x = screenX(particle.x), y = screenTop(particle.y, height: particle.size)
            let fadeOut = Float(particle.life / particle.maxLife)
            if fadeOut > 0.5 || bayer(x, y) < fadeOut * 2 {
                canvas.fillRect(x: x, y: y, width: particle.size, height: particle.size, particle.color)
            }
        }
    }

    // MARK: HUD

    private func drawHUD(_ game: SpellrunGame, _ canvas: PixelCanvas, levels: BandLevels, showMeters: Bool) {
        let shadow = SpellrunArt.outline
        // Top right, clear of the window's title-bar buttons on the left.
        let count = String(format: "%03d", min(game.motesCollected, 999))
        let countX = width - 5 - (count.count * 4 - 1)
        canvas.draw(SpellrunArt.mote[0], x: countX - 8, y: 5)
        canvas.text(count, x: countX + 1, y: 6, shadow)
        canvas.text(count, x: countX, y: 5, rgb(0xFFF4C8))

        if game.bannerTime > 0 {
            let name = game.biome.name
            let textWidth = name.count * 4 - 1
            let x = (width - textWidth) / 2, y = 18
            let alpha = Float(min(game.bannerTime, 1, (3 - game.bannerTime) * 3))
            for (offset, color) in [(1, shadow), (0, rgb(0xFFFFFF))] {
                for (i, character) in name.uppercased().enumerated() where bayer(x + i * 4, y) < alpha {
                    canvas.text(String(character), x: x + i * 4 + offset, y: y + offset, color)
                }
            }
        }

        if showMeters {
            let colors = [rgb(0xFF5C7A), rgb(0x74FFAD), rgb(0x7CC8FF)]
            for band in 0..<3 {
                let x = 5 + band * 5, bottom = height - 5, full = 28
                let filled = Int(Float(full) * levels[band])
                canvas.fillRect(x: x, y: bottom - full, width: 3, height: full, rgb(0x15101E))
                canvas.fillRect(x: x, y: bottom - filled, width: 3, height: filled, colors[band])
            }
        }
    }

    // MARK: Noise

    private func hash(_ x: Int, _ y: Int) -> Double {
        var h = UInt64(bitPattern: Int64(x)) &* 0x9E37_79B9_7F4A_7C15 ^ UInt64(bitPattern: Int64(y)) &* 0xC2B2_AE3D_27D4_EB4F
        h ^= h >> 29
        h = h &* 0xBF58_476D_1CE4_E5B9
        h ^= h >> 32
        return Double(h >> 11) / Double(1 << 53)
    }

    /// Smooth 1D value noise in about 0...1.
    private func noise(_ u: Double) -> Double {
        let i = floor(u), f = u - i
        let t = f * f * (3 - 2 * f)
        return hash(Int(i), 0) * (1 - t) + hash(Int(i) + 1, 0) * t
    }
}
