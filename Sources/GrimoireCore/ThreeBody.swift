import Foundation
import simd

/// Newtonian three-body gravity in 2D, integrated with velocity Verlet (symplectic, so energy stays stable).
///
/// Built to be pushed around by music, so it has guard rails a pure simulation wouldn't:
/// - softening keeps close encounters finite,
/// - a soft wall pulls bodies back if they stray past `wallRadius`,
/// - kinetic energy is capped so constant pumping can't spin the system up forever,
/// - an optional thermostat eases total energy toward `targetEnergy`, so the orbit keeps its size instead of
///   collapsing (when gravity is pumped) or flying apart,
/// - the center of mass is pinned to the origin.
public struct ThreeBodySystem: Sendable {
    public struct Body: Sendable {
        public var position: SIMD2<Double>
        public var velocity: SIMD2<Double>
        public var mass: Double

        public init(position: SIMD2<Double>, velocity: SIMD2<Double>, mass: Double) {
            self.position = position
            self.velocity = velocity
            self.mass = mass
        }
    }

    public var bodies: [Body]
    public var gravity: Double = 1
    public var softening: Double = 0.05
    public var wallRadius: Double = 2.5
    public var wallStiffness: Double = 4
    /// Optional gentle harmonic bowl (acceleration -k·r) that keeps all three bodies mixing instead of settling
    /// into a tight pair with the third flung out. 0 is pure Newtonian gravity.
    public var trapStrength: Double = 0
    public var maxKineticEnergy: Double = 6
    /// Total energy the thermostat steers toward, or nil to leave energy alone.
    public var targetEnergy: Double?
    /// How quickly the thermostat corrects, per unit of simulated time.
    public var thermostatRate: Double = 0.6

    public init(bodies: [Body]) {
        self.bodies = bodies
    }

    /// The Chenciner–Montgomery figure-eight: three equal masses chasing each other on one stable loop.
    public static func figureEight() -> ThreeBodySystem {
        let position = SIMD2(0.97000436, -0.24308753)
        let velocity = SIMD2(-0.93240737, -0.86473146)
        return ThreeBodySystem(bodies: [
            Body(position: position, velocity: -velocity / 2, mass: 1),
            Body(position: -position, velocity: -velocity / 2, mass: 1),
            Body(position: .zero, velocity: velocity, mass: 1),
        ])
    }

    public mutating func setMasses(_ masses: [Double]) {
        for i in bodies.indices where i < masses.count { bodies[i].mass = masses[i] }
    }

    /// Rotates the whole configuration (positions and velocities) about the origin.
    public mutating func rotate(by angle: Double) {
        let c = cos(angle), s = sin(angle)
        func turn(_ v: SIMD2<Double>) -> SIMD2<Double> { SIMD2(c * v.x - s * v.y, s * v.x + c * v.y) }
        for i in bodies.indices {
            bodies[i].position = turn(bodies[i].position)
            bodies[i].velocity = turn(bodies[i].velocity)
        }
    }

    /// Nudges each velocity by up to ±`amount` (relative), which sends the figure-eight into chaos.
    public mutating func perturb(_ amount: Double) {
        for i in bodies.indices {
            bodies[i].velocity *= SIMD2(1 + .random(in: -amount...amount), 1 + .random(in: -amount...amount))
        }
        recenter()
    }

    /// Advances the simulation by `time`, in sub-steps no longer than `maxStep`.
    public mutating func advance(by time: Double, maxStep: Double = 0.002) {
        guard time > 0 else { return }
        let steps = min(Int((time / maxStep).rounded(.up)), 500)
        let dt = time / Double(steps)
        var acceleration = accelerations()
        for _ in 0..<steps {
            for i in bodies.indices {
                bodies[i].velocity += acceleration[i] * (dt / 2)
                bodies[i].position += bodies[i].velocity * dt
            }
            acceleration = accelerations()
            for i in bodies.indices {
                bodies[i].velocity += acceleration[i] * (dt / 2)
            }
        }
        applyThermostat(over: time)
        capKineticEnergy()
        recenter()
    }

    public var kineticEnergy: Double {
        bodies.reduce(0) { $0 + 0.5 * $1.mass * simd_length_squared($1.velocity) }
    }

    /// Kinetic + softened gravitational + wall potential energy.
    public var totalEnergy: Double {
        var potential = 0.0
        for i in bodies.indices {
            for j in bodies.indices where j > i {
                let distance = (simd_length_squared(bodies[j].position - bodies[i].position) + softening * softening).squareRoot()
                potential -= gravity * bodies[i].mass * bodies[j].mass / distance
            }
            let overshoot = max(0, simd_length(bodies[i].position) - wallRadius)
            potential += 0.5 * wallStiffness * bodies[i].mass * overshoot * overshoot
            potential += 0.5 * trapStrength * bodies[i].mass * simd_length_squared(bodies[i].position)
        }
        return kineticEnergy + potential
    }

    public var momentum: SIMD2<Double> {
        bodies.reduce(.zero) { $0 + $1.mass * $1.velocity }
    }

    /// Distance of the farthest body from the center.
    public var extent: Double {
        bodies.map { simd_length($0.position) }.max() ?? 0
    }

    private func accelerations() -> [SIMD2<Double>] {
        var result = [SIMD2<Double>](repeating: .zero, count: bodies.count)
        let soft2 = softening * softening
        for i in bodies.indices {
            for j in bodies.indices where j > i {
                let delta = bodies[j].position - bodies[i].position
                let r2 = simd_length_squared(delta) + soft2
                let strength = gravity / (r2 * r2.squareRoot())
                result[i] += delta * (strength * bodies[j].mass)
                result[j] -= delta * (strength * bodies[i].mass)
            }
            result[i] -= bodies[i].position * trapStrength
            let r = simd_length(bodies[i].position)
            if r > wallRadius {
                result[i] -= bodies[i].position / r * (wallStiffness * (r - wallRadius))
            }
        }
        return result
    }

    /// Rescales velocities so total energy moves part of the way toward `targetEnergy`.
    private mutating func applyThermostat(over time: Double) {
        guard let targetEnergy else { return }
        let kinetic = kineticEnergy
        guard kinetic > 1e-9 else { return }
        let correction = (targetEnergy - totalEnergy) * min(1, thermostatRate * time)
        let desired = max(kinetic + correction, 0.05)
        let scale = (desired / kinetic).squareRoot()
        for i in bodies.indices { bodies[i].velocity *= scale }
    }

    private mutating func capKineticEnergy() {
        let kinetic = kineticEnergy
        guard kinetic > maxKineticEnergy else { return }
        let scale = (maxKineticEnergy / kinetic).squareRoot()
        for i in bodies.indices { bodies[i].velocity *= scale }
    }

    private mutating func recenter() {
        let totalMass = bodies.reduce(0) { $0 + $1.mass }
        guard totalMass > 0 else { return }
        let center = bodies.reduce(SIMD2<Double>.zero) { $0 + $1.mass * $1.position } / totalMass
        let drift = momentum / totalMass
        for i in bodies.indices {
            bodies[i].position -= center
            bodies[i].velocity -= drift
        }
    }
}
