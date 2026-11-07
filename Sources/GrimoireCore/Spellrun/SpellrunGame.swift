import Foundation

public enum EnemyKind: Sendable {
    case slime, bat
}

/// One 16 px wide slice of the world. Rows count up from the bottom.
public struct Column: Sendable {
    /// Solid ground tiles from the bottom; 0 is a pit.
    public var ground: Int
    /// Row of a one-way floating platform, or -1.
    public var platform = -1
    /// Row of a solid rune block, or -1.
    public var block = -1
    /// Row of a collectible mote, or -1.
    public var mote = -1
    public var enemy: EnemyKind?
    public var enemyRow = 0
    /// A beat lantern post that flashes with the bass.
    public var lantern = false
    /// 0 none, 1 grass tuft, 2 flower, 3 crystal or rock.
    public var decor: UInt8 = 0
    public var biome = 0

    public init(ground: Int) {
        self.ground = ground
    }
}

/// Endless level generator. Every gap and step it produces is one the apprentice can always clear at minimum
/// speed. `energy` (0...1, driven by the mids) shifts it from long calm runs toward gaps, hills, platforms and blocks.
public struct LevelGenerator: Sendable {
    public static let maxGap = 3
    public static let maxStepUp = 2
    public static let minHeight = 2
    public static let maxHeight = 6

    public var energy: Double = 0.3
    public var biome = 0
    private var rng: SplitMix64
    private var height = 3
    private var queue: [Column] = []
    private var sinceLantern = 0

    public init(seed: UInt64) {
        rng = SplitMix64(seed: seed)
        _ = emitFlat(10)   // a safe runway to start on
    }

    public mutating func next() -> Column {
        while queue.isEmpty { generateSegment() }
        return queue.removeFirst()
    }

    private mutating func generateSegment() {
        let e = energy
        let weights = [3.0 - 2.0 * e, 1.5 + 0.5 * e, 0.8 + 1.6 * e, 0.8 + 1.0 * e, 0.8 + 0.7 * e]
        var roll = rng.unit() * weights.reduce(0, +)
        var choice = 0
        while choice < weights.count - 1, roll >= weights[choice] {
            roll -= weights[choice]
            choice += 1
        }
        switch choice {
        case 0: flatRun()
        case 1: hill()
        case 2: gap()
        case 3: platforms()
        default: blocks()
        }
        if choice != 2, rng.chance(0.10 + 0.2 * e) { addBat() }
    }

    private mutating func emitFlat(_ count: Int) -> Range<Int> {
        let start = queue.count
        for _ in 0..<count {
            var column = Column(ground: height)
            column.biome = biome
            sinceLantern += 1
            if sinceLantern >= 11, rng.chance(0.4) {
                column.lantern = true
                sinceLantern = 0
            } else if rng.chance(0.35) {
                column.decor = UInt8(rng.int(1...3))
            }
            queue.append(column)
        }
        return start..<queue.count
    }

    private mutating func flatRun() {
        let run = emitFlat(rng.int(4...9))
        if run.count >= 5, rng.chance(0.25 + 0.3 * energy) {
            queue[run.lowerBound + run.count / 2].enemy = .slime
        }
        if rng.chance(0.4) {
            let start = run.lowerBound + 1
            for (offset, lift) in [1, 2, 2, 1].enumerated() where start + offset < run.upperBound {
                queue[start + offset].mote = height + lift
            }
        }
    }

    private mutating func hill() {
        let up = height <= Self.minHeight || (height < Self.maxHeight && rng.chance(0.5))
        let step = up ? rng.int(1...(energy > 0.5 ? Self.maxStepUp : 1)) : rng.int(1...2)
        height = min(max(height + (up ? step : -step), Self.minHeight), Self.maxHeight)
        _ = emitFlat(rng.int(3...6))
    }

    private mutating func gap() {
        let width = rng.int(1...(energy > 0.55 ? Self.maxGap : 2))
        let start = queue.count
        for _ in 0..<width {
            var pit = Column(ground: 0)
            pit.biome = biome
            queue.append(pit)
        }
        // A reward arcing over the gap.
        if rng.chance(0.5) { queue[start + width / 2].mote = height + 3 }
        // A wide gap only lands level or lower; a narrow one may land one step up.
        let change = width >= Self.maxGap ? rng.int(-1...0) : rng.int(-1...1)
        height = min(max(height + change, Self.minHeight), Self.maxHeight)
        _ = emitFlat(rng.int(3...5))
    }

    private mutating func platforms() {
        let run = emitFlat(rng.int(7...10))
        let length = rng.int(3...min(5, run.count - 4))
        let first = run.lowerBound + 2
        for index in first..<(first + length) {
            queue[index].platform = height + 2
            if (index - first).isMultiple(of: 2) { queue[index].mote = height + 3 }
        }
    }

    private mutating func blocks() {
        let run = emitFlat(rng.int(6...9))
        let count = rng.int(1...3)
        for i in 0..<count {
            let index = run.lowerBound + 2 + i * 2
            guard index < run.upperBound - 1 else { break }
            queue[index].block = height + 3
            queue[index].mote = height + 4
        }
    }

    private mutating func addBat() {
        guard let index = queue.indices.last(where: { queue[$0].ground > 0 && queue[$0].platform < 0 && queue[$0].block < 0 }) else { return }
        queue[index].enemy = .bat
        queue[index].enemyRow = queue[index].ground + 3
    }
}

/// What the music does to the game this frame.
public struct SpellrunInput: Sendable {
    public var low: Float
    public var mid: Float
    public var high: Float
    /// A kick just landed (rising edge of the low-band hit).
    public var kick: Bool
    /// A kick strong enough for a ground pound.
    public var bigKick: Bool
    /// A high-band transient: cast a spell, drop a mote.
    public var sparkle: Bool
    /// It just got loud after a quiet stretch.
    public var drop: Bool

    public init(low: Float = 0, mid: Float = 0, high: Float = 0, kick: Bool = false, bigKick: Bool = false, sparkle: Bool = false, drop: Bool = false) {
        self.low = low
        self.mid = mid
        self.high = high
        self.kick = kick
        self.bigKick = bigKick
        self.sparkle = sparkle
        self.drop = drop
    }
}

/// Spellrun: a little apprentice auto-running through an endless generated pixel world, played by the music.
/// World units are pixels; y points up from the bottom of the world.
public final class SpellrunGame {
    public static let tile = 16
    static let gravity = 1000.0
    static let jumpSpeed = 340.0
    static let hopSpeed = 230.0
    static let poundSpeed = -540.0
    static let minRunSpeed = 95.0
    static let heroWidth = 10.0
    static let heroHeight = 14.0

    public struct Hero {
        public var x = 48.0, y = 48.0, vx = minRunSpeed, vy = 0.0
        public var grounded = true
        public var pounding = false
        public var blink = 0.0
        public var stride = 0.0
        /// The staff boost (a magic double jump) recharges on landing or after a short cooldown.
        public var canBoost = true
        var boostCooldown = 0.0
        public var boostGlow = 0.0
        var castCooldown = 0.0
        var stuck = 0.0
    }

    struct Enemy {
        var kind: EnemyKind
        var x: Double
        var y: Double
        var baseY: Double
        var vy = 0.0
        var phase: Double
        var squish: Double = 0   // > 0 while showing the squished frame
        var dead: Bool { squish > 0 }
    }

    struct Mote {
        var x: Double
        var y: Double
        var phase: Double
        var rising = 0.0
    }

    struct Bolt {
        var x: Double
        var y: Double
        var life: Double
    }

    struct Particle {
        var x = 0.0, y = 0.0, vx = 0.0, vy = 0.0
        var life = 0.0, maxLife = 1.0
        var color: UInt32 = 0
        var size = 1
        var gravity = 0.0
    }

    public private(set) var hero = Hero()
    public private(set) var cameraX = 0.0
    public private(set) var cameraY = 0.0
    public private(set) var motesCollected = 0
    public private(set) var respawns = 0
    public private(set) var time = 0.0
    public private(set) var biomeIndex = 0
    public private(set) var previousBiome = 0
    /// 0...1 dissolve from the previous biome's backdrop to the current one.
    public private(set) var biomeFade = 1.0
    /// Seconds left showing the biome name banner.
    public private(set) var bannerTime = 3.0
    public private(set) var shake = 0.0
    public private(set) var input = SpellrunInput()

    /// Running totals of what the music made happen; handy for tests and tuning.
    public struct Stats: Sendable {
        public var beatHops = 0, pounds = 0, boosts = 0, spells = 0, stomps = 0, blocksShattered = 0
    }
    public private(set) var stats = Stats()

    private(set) var columns: [Column] = []
    private(set) var firstColumn = 0
    private(set) var enemies: [Enemy] = []
    private(set) var motes: [Mote] = []
    private(set) var bolts: [Bolt] = []
    private(set) var particles = [Particle](repeating: Particle(), count: 320)
    private var particleCursor = 0
    private var generator: LevelGenerator
    private var rng: SplitMix64
    private var biomeTimer: Double
    private var lastBiomeChange = 0.0
    private(set) var viewWidth = 256.0
    private(set) var viewHeight = 160.0

    public init(seed: UInt64 = .random(in: 0...UInt64.max)) {
        generator = LevelGenerator(seed: seed)
        rng = SplitMix64(seed: seed &+ 1)
        biomeTimer = 55
        let startBiome = Int(seed % UInt64(Biome.all.count))
        generator.biome = startBiome
        biomeIndex = startBiome
        previousBiome = startBiome
        ensureColumns()
    }

    public var biome: Biome { Biome.all[biomeIndex] }

    func column(_ index: Int) -> Column? {
        let local = index - firstColumn
        return local >= 0 && local < columns.count ? columns[local] : nil
    }

    // MARK: Step

    public func step(dt: Double, input: SpellrunInput, viewWidth: Double, viewHeight: Double) {
        guard dt > 0 else { return }
        self.input = input
        self.viewWidth = viewWidth
        self.viewHeight = viewHeight
        time += dt

        generator.energy += (Double(input.mid) - generator.energy) * (1 - exp(-dt / 3))
        biomeTimer -= dt
        if biomeTimer <= 0 || (input.drop && time - lastBiomeChange > 15) {
            generator.biome = (generator.biome + 1) % Biome.all.count
            biomeTimer = 50 + rng.unit() * 30
            lastBiomeChange = time
        }

        let targetSpeed = Self.minRunSpeed + 80 * Double(input.mid)
        hero.vx += (targetSpeed - hero.vx) * (1 - exp(-dt / 0.8))
        updateHero(dt: dt)
        updateEnemies(dt: dt)
        updateBolts(dt: dt)
        updateMotes(dt: dt)
        updateParticles(dt: dt)

        if input.sparkle, rng.chance(0.6) {
            let x = hero.x + viewWidth * (0.5 + 0.3 * rng.unit())
            let ground = Double((column(Int(x) / Self.tile)?.ground ?? 3) * Self.tile)
            motes.append(Mote(x: x, y: ground + Double(Self.tile) * (2 + 2 * rng.unit()), phase: rng.unit() * 6))
        }

        let targetX = hero.x - viewWidth * 0.3
        cameraX += (targetX - cameraX) * (1 - exp(-dt / 0.12))
        let targetY = max(0, hero.y - viewHeight * 0.35)
        cameraY += (targetY - cameraY) * (1 - exp(-dt / 0.45))

        let center = Int((cameraX + viewWidth / 2) / Double(Self.tile))
        if let biomeHere = column(center)?.biome, biomeHere != biomeIndex {
            previousBiome = biomeIndex
            biomeIndex = biomeHere
            biomeFade = 0
            bannerTime = 3
        }
        biomeFade = min(1, biomeFade + dt / 1.5)
        bannerTime = max(0, bannerTime - dt)
        shake = max(0, shake - dt * 3)

        ensureColumns()
        while firstColumn < Int(cameraX / Double(Self.tile)) - 4, !columns.isEmpty {
            columns.removeFirst()
            firstColumn += 1
        }
        enemies.removeAll { $0.x < cameraX - 40 || $0.y < -60 }
        motes.removeAll { $0.x < cameraX - 20 }
    }

    private func ensureColumns() {
        let needed = Int((cameraX + viewWidth) / Double(Self.tile)) + 10
        while firstColumn + columns.count < needed {
            let index = firstColumn + columns.count
            let column = generator.next()
            columns.append(column)
            let x = Double(index * Self.tile)
            if column.mote >= 0 {
                motes.append(Mote(x: x + 5.5, y: Double(column.mote * Self.tile) + 5, phase: Double(index)))
            }
            switch column.enemy {
            case .slime:
                let y = Double(column.ground * Self.tile)
                enemies.append(Enemy(kind: .slime, x: x + 2, y: y, baseY: y, phase: Double(index)))
            case .bat:
                let y = Double(column.enemyRow * Self.tile)
                enemies.append(Enemy(kind: .bat, x: x + 2, y: y, baseY: y, phase: Double(index)))
            case nil:
                break
            }
        }
    }

    // MARK: World queries

    private func isSolid(_ col: Int, _ row: Int) -> Bool {
        guard let c = column(col) else { return row < 3 }
        return row < c.ground || row == c.block
    }

    private func columnRange(x: Double) -> ClosedRange<Int> {
        Int(floor((x + 1) / Double(Self.tile)))...Int(floor((x + Self.heroWidth - 1) / Double(Self.tile)))
    }

    /// The highest surface the hero's feet cross while falling from `from` to `to`, if any.
    private func landing(x: Double, from: Double, to: Double) -> Double? {
        var best: Double?
        for col in columnRange(x: x) {
            guard let c = column(col) else { continue }
            var tops = [Double(c.ground * Self.tile)]
            if c.platform >= 0 { tops.append(Double((c.platform + 1) * Self.tile)) }
            if c.block >= 0 { tops.append(Double((c.block + 1) * Self.tile)) }
            for top in tops where c.ground > 0 || top > 0 {
                if from >= top - 0.5, to <= top { best = max(best ?? -1, top) }
            }
        }
        return best
    }

    /// The column and row of a rune block the hero's head hits while rising, if any.
    private func ceiling(x: Double, from: Double, to: Double) -> (col: Int, bottom: Double)? {
        for col in columnRange(x: x) {
            guard let c = column(col), c.block >= 0 else { continue }
            let bottom = Double(c.block * Self.tile)
            if from <= bottom + 0.5, to > bottom { return (col, bottom) }
        }
        return nil
    }

    private func surface(at x: Double) -> Double {
        Double((column(Int(floor(x / Double(Self.tile))))?.ground ?? 0) * Self.tile)
    }

    // MARK: Hero

    private func updateHero(dt: Double) {
        var h = hero
        let tile = Double(Self.tile)

        if h.grounded {
            let front = h.x + Self.heroWidth
            let nextCol = Int(floor(front / tile)) + 1
            let toEdge = Double(nextCol) * tile - front
            let next = column(nextCol)
            let slimeAhead = enemies.contains {
                $0.kind == .slime && !$0.dead && $0.x > h.x && $0.x - front < 30 && abs($0.y - h.y) < 8
            }
            var jump: Double?
            // Landing right at a gap's edge can leave the front foot already over the pit, so check that column too.
            let underFront = column(Int(floor((front - 0.01) / tile)))
            if let underFront, underFront.ground == 0 {
                jump = Self.jumpSpeed
            } else if let next, next.ground == 0, toEdge < 8 {
                jump = Self.jumpSpeed
            } else if let next, Double(next.ground) * tile > h.y + 1, toEdge < 10 {
                jump = Self.jumpSpeed
            } else if slimeAhead {
                jump = Self.jumpSpeed
            } else if let next, next.platform >= 0, (column(nextCol - 1)?.platform ?? -1) < 0, toEdge < 6,
                      Double(next.platform) * tile > h.y {
                jump = Self.jumpSpeed
            } else if input.kick, safeToHop(from: nextCol - 1, y: h.y) {
                jump = Self.hopSpeed
                stats.beatHops += 1
            } else if h.stuck > 0.4 {
                jump = Self.jumpSpeed
            }
            if let jump {
                h.vy = jump
                h.grounded = false
                h.stuck = 0
                dust(x: h.x + Self.heroWidth / 2, y: h.y, count: 4)
            }
        } else {
            let center = h.x + Self.heroWidth / 2
            let overPit = columnRange(x: h.x).allSatisfy { (column($0)?.ground ?? 1) == 0 }
            if overPit, h.vy < 0, h.canBoost {
                // About to drop below the far ledge? Staff boost: a magic double jump out of the gap.
                let col = Int(floor(center / tile))
                let ledge = (col...(col + 3)).compactMap { column($0)?.ground }.max() ?? 0
                if h.y < Double(ledge) * tile + 6 {
                    h.vy = Self.jumpSpeed * 0.9
                    h.canBoost = false
                    h.boostCooldown = 0.45
                    h.pounding = false
                    h.boostGlow = 1
                    stats.boosts += 1
                    burst(x: center, y: h.y, color: rgb(0x74FFAD), count: 14, speed: 70)
                }
            } else if input.bigKick, !h.pounding, !overPit, h.vy < 80, h.y - surface(at: center) > 24 {
                h.vy = Self.poundSpeed
                h.pounding = true
                stats.pounds += 1
            }
        }
        h.boostGlow = max(0, h.boostGlow - dt * 2)
        h.boostCooldown -= dt
        if !h.canBoost, h.boostCooldown <= 0 { h.canBoost = true }

        h.castCooldown -= dt
        if input.sparkle, h.castCooldown <= 0 {
            bolts.append(Bolt(x: h.x + 11, y: h.y + 14, life: 0.9))
            h.castCooldown = 0.3
            stats.spells += 1
            burst(x: h.x + 11, y: h.y + 14, color: rgb(0x9FFFD0), count: 5, speed: 40)
        }

        // Horizontal: run, stopping at walls.
        var x = h.x + h.vx * dt
        let frontCol = Int(floor((x + Self.heroWidth) / tile))
        let lowRow = Int(floor((h.y + 1) / tile)), highRow = Int(floor((h.y + Self.heroHeight - 1) / tile))
        if (lowRow...max(lowRow, highRow)).contains(where: { isSolid(frontCol, $0) }) {
            x = Double(frontCol) * tile - Self.heroWidth - 0.01
            h.stuck += dt
        } else {
            h.stuck = 0
        }
        h.stride += x - h.x
        h.x = x

        // Vertical: gravity, landing, head bonks.
        h.vy = max(h.vy - Self.gravity * dt, -720)
        let previousY = h.y
        var y = h.y + h.vy * dt
        let wasGrounded = h.grounded
        h.grounded = false
        if h.vy <= 0, let top = landing(x: h.x, from: previousY, to: y) {
            y = top
            if h.pounding {
                groundPound(at: h.x + Self.heroWidth / 2, y: top)
                h.pounding = false
            } else if !wasGrounded, h.vy < -300 {
                dust(x: h.x + Self.heroWidth / 2, y: top, count: 3)
            }
            h.vy = 0
            h.grounded = true
            h.canBoost = true
        } else if h.vy > 0, let hit = ceiling(x: h.x, from: previousY + Self.heroHeight, to: y + Self.heroHeight) {
            y = hit.bottom - Self.heroHeight
            h.vy = 0
            shatterBlock(hit.col)
        }
        h.y = y

        // Stomping and bumping into enemies.
        for i in enemies.indices where !enemies[i].dead {
            let e = enemies[i]
            let width = 12.0, height = e.kind == .slime ? 9.0 : 8.0
            let overlaps = h.x < e.x + width && h.x + Self.heroWidth > e.x && h.y < e.y + height && h.y + Self.heroHeight > e.y
            guard overlaps else { continue }
            if h.vy < 0, h.y > e.y + height * 0.4 {
                enemies[i].squish = 0.35
                stats.stomps += 1
                h.vy = 260
                h.grounded = false
                burst(x: e.x + 6, y: e.y + 4, color: e.kind == .slime ? rgb(0x46D3A0) : rgb(0x9E7CC8), count: 8, speed: 70)
            } else if h.blink <= 0 {
                enemies[i].squish = 0.35
                h.blink = 0.6
                burst(x: e.x + 6, y: e.y + 4, color: rgb(0xFFFFFF), count: 6, speed: 50)
            }
        }
        h.blink = max(0, h.blink - dt)

        // Safety net: if the hero ever falls into a pit, pop them back onto the next solid ground.
        if h.y < -40 {
            var col = Int(floor(h.x / tile)) + 1
            while (column(col)?.ground ?? 1) == 0 { col += 1 }
            h.x = Double(col) * tile + 2
            h.y = Double((column(col)?.ground ?? 3) * Self.tile)
            h.vy = 0
            h.grounded = true
            h.pounding = false
            respawns += 1
            burst(x: h.x + 5, y: h.y + 7, color: rgb(0xE6FFF0), count: 12, speed: 60)
        }
        hero = h
    }

    private func safeToHop(from col: Int, y: Double) -> Bool {
        let level = Int((y / Double(Self.tile)).rounded())
        for offset in 0...4 {
            guard let c = column(col + offset), c.ground == level else { return false }
        }
        return true
    }

    private func groundPound(at x: Double, y: Double) {
        shake = 1
        dust(x: x, y: y, count: 14)
        for i in enemies.indices where !enemies[i].dead && abs(enemies[i].x - x) < 110 {
            enemies[i].squish = 0.35
            burst(x: enemies[i].x + 6, y: enemies[i].y + 4, color: rgb(0xFFFFFF), count: 5, speed: 60)
        }
    }

    private func shatterBlock(_ col: Int) {
        let local = col - firstColumn
        guard local >= 0, local < columns.count, columns[local].block >= 0 else { return }
        let row = columns[local].block
        columns[local].block = -1
        stats.blocksShattered += 1
        let x = Double(col * Self.tile) + 8, y = Double(row * Self.tile) + 8
        burst(x: x, y: y, color: Biome.all[columns[local].biome].blockGlow, count: 16, speed: 90)
        motes.append(Mote(x: x - 2.5, y: y, phase: 0, rising: 0.4))
    }

    // MARK: Everything else

    private func updateEnemies(dt: Double) {
        for i in enemies.indices {
            if enemies[i].dead {
                enemies[i].squish -= dt
                if enemies[i].squish <= 0 { enemies[i].y = -100 }
                continue
            }
            switch enemies[i].kind {
            case .slime:
                enemies[i].x -= 12 * dt
                let ground = surface(at: enemies[i].x + 6)
                if input.kick, enemies[i].y <= ground + 0.5 { enemies[i].vy = 120 }   // slimes dance too
                enemies[i].vy -= Self.gravity * dt
                enemies[i].y += enemies[i].vy * dt
                // Over a pit there's no ground, so the slime just falls out of the world.
                if ground > 0, enemies[i].y < ground {
                    enemies[i].y = ground
                    enemies[i].vy = 0
                }
            case .bat:
                enemies[i].x -= 22 * dt
                enemies[i].y = enemies[i].baseY + sin(time * 3 + enemies[i].phase) * 6
            }
        }
        enemies.removeAll { $0.y <= -100 }
    }

    private func updateBolts(dt: Double) {
        for i in bolts.indices {
            bolts[i].x += 280 * dt
            bolts[i].life -= dt
            spawnParticle(Particle(x: bolts[i].x, y: bolts[i].y, vx: -20, vy: .random(in: -10...10),
                                   life: 0.25, maxLife: 0.25, color: rgb(0x74FFAD), size: 1))
            let col = Int(floor(bolts[i].x / Double(Self.tile)))
            if let c = column(col), c.block >= 0, Int(floor(bolts[i].y / Double(Self.tile))) == c.block {
                shatterBlock(col)
                bolts[i].life = 0
            }
            for j in enemies.indices where !enemies[j].dead {
                let e = enemies[j]
                if bolts[i].x > e.x, bolts[i].x < e.x + 12, bolts[i].y > e.y - 2, bolts[i].y < e.y + 10 {
                    enemies[j].squish = 0.35
                    bolts[i].life = 0
                    burst(x: e.x + 6, y: e.y + 4, color: rgb(0xFFD84D), count: 10, speed: 80)
                    motes.append(Mote(x: e.x + 4, y: e.y + 4, phase: 0, rising: 0.3))
                }
            }
        }
        bolts.removeAll { $0.life <= 0 }
    }

    private func updateMotes(dt: Double) {
        let h = hero
        motes.removeAll { mote in
            let bob = sin(time * 3 + mote.phase) * 2
            let hit = mote.x + 5 > h.x - 2 && mote.x < h.x + Self.heroWidth + 2
                && mote.y + bob + 5 > h.y - 2 && mote.y + bob < h.y + Self.heroHeight + 2
            if hit {
                motesCollected += 1
                burst(x: mote.x + 2.5, y: mote.y + 2.5, color: rgb(0xFFD84D), count: 6, speed: 45)
            }
            return hit
        }
        for i in motes.indices where motes[i].rising > 0 {
            motes[i].y += 40 * dt
            motes[i].rising -= dt
        }
    }

    private func updateParticles(dt: Double) {
        for i in particles.indices where particles[i].life > 0 {
            particles[i].life -= dt
            particles[i].vy -= particles[i].gravity * dt
            particles[i].x += particles[i].vx * dt
            particles[i].y += particles[i].vy * dt
        }
    }

    /// Reuses the particle pool round-robin, overwriting the oldest when it's full.
    private func spawnParticle(_ particle: Particle) {
        particles[particleCursor] = particle
        particleCursor = (particleCursor + 1) % particles.count
    }

    private func burst(x: Double, y: Double, color: UInt32, count: Int, speed: Double) {
        for _ in 0..<count {
            let angle = rng.unit() * 2 * .pi
            let v = speed * (0.4 + 0.6 * rng.unit())
            spawnParticle(Particle(x: x, y: y, vx: cos(angle) * v, vy: sin(angle) * v, life: 0.5, maxLife: 0.5,
                                   color: color, size: rng.chance(0.3) ? 2 : 1, gravity: 120))
        }
    }

    private func dust(x: Double, y: Double, count: Int) {
        let color = mix(biome.groundTop, rgb(0xFFFFFF), 0.4)
        for _ in 0..<count {
            let direction = rng.chance(0.5) ? -1.0 : 1.0
            spawnParticle(Particle(x: x, y: y + 1, vx: direction * (20 + 50 * rng.unit()), vy: 10 + 30 * rng.unit(),
                                   life: 0.35, maxLife: 0.35, color: color, size: 1, gravity: 60))
        }
    }
}
