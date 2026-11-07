use glam::DVec2;

#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub position: DVec2,
    pub velocity: DVec2,
    pub mass: f64,
}

/// Newtonian three-body gravity in 2D, integrated with velocity Verlet (symplectic, so energy stays stable).
///
/// Built to be pushed around by music, so it has guard rails a pure simulation wouldn't:
/// - softening keeps close encounters finite,
/// - a soft wall pulls bodies back if they stray past `wall_radius`,
/// - kinetic energy is capped so constant pumping can't spin the system up forever,
/// - an optional thermostat eases total energy toward `target_energy`, so the orbit keeps its size instead of
///   collapsing (when gravity is pumped) or flying apart,
/// - the center of mass is pinned to the origin.
#[derive(Clone, Debug)]
pub struct ThreeBodySystem {
    pub bodies: Vec<Body>,
    pub gravity: f64,
    pub softening: f64,
    pub wall_radius: f64,
    pub wall_stiffness: f64,
    /// Optional gentle harmonic bowl (acceleration -k·r) that keeps all three bodies mixing instead of settling
    /// into a tight pair with the third flung out. 0 is pure Newtonian gravity.
    pub trap_strength: f64,
    pub max_kinetic_energy: f64,
    /// Total energy the thermostat steers toward, or None to leave energy alone.
    pub target_energy: Option<f64>,
    /// How quickly the thermostat corrects, per unit of simulated time.
    pub thermostat_rate: f64,
}

impl ThreeBodySystem {
    pub fn new(bodies: Vec<Body>) -> Self {
        ThreeBodySystem {
            bodies,
            gravity: 1.0,
            softening: 0.05,
            wall_radius: 2.5,
            wall_stiffness: 4.0,
            trap_strength: 0.0,
            max_kinetic_energy: 6.0,
            target_energy: None,
            thermostat_rate: 0.6,
        }
    }

    /// The Chenciner–Montgomery figure-eight: three equal masses chasing each other on one stable loop.
    pub fn figure_eight() -> Self {
        let position = DVec2::new(0.97000436, -0.24308753);
        let velocity = DVec2::new(-0.93240737, -0.86473146);
        ThreeBodySystem::new(vec![
            Body { position, velocity: -velocity / 2.0, mass: 1.0 },
            Body { position: -position, velocity: -velocity / 2.0, mass: 1.0 },
            Body { position: DVec2::ZERO, velocity, mass: 1.0 },
        ])
    }

    pub fn set_masses(&mut self, masses: &[f64]) {
        for (body, &mass) in self.bodies.iter_mut().zip(masses) {
            body.mass = mass;
        }
    }

    /// Rotates the whole configuration (positions and velocities) about the origin.
    pub fn rotate(&mut self, angle: f64) {
        let turn = DVec2::from_angle(angle);
        for body in &mut self.bodies {
            body.position = turn.rotate(body.position);
            body.velocity = turn.rotate(body.velocity);
        }
    }

    /// Nudges each velocity by up to ±`amount` (relative), which sends the figure-eight into chaos.
    pub fn perturb(&mut self, amount: f64) {
        let jitter = || 1.0 + (fastrand::f64() * 2.0 - 1.0) * amount;
        for body in &mut self.bodies {
            body.velocity *= DVec2::new(jitter(), jitter());
        }
        self.recenter();
    }

    /// Advances the simulation by `time`, in sub-steps no longer than `max_step`.
    pub fn advance(&mut self, time: f64) {
        self.advance_with_step(time, 0.002);
    }

    pub fn advance_with_step(&mut self, time: f64, max_step: f64) {
        if !(time > 0.0) {
            return;
        }
        let steps = ((time / max_step).ceil() as usize).clamp(1, 500);
        let dt = time / steps as f64;
        let mut acceleration = self.accelerations();
        for _ in 0..steps {
            for (body, a) in self.bodies.iter_mut().zip(&acceleration) {
                body.velocity += *a * (dt / 2.0);
                body.position += body.velocity * dt;
            }
            acceleration = self.accelerations();
            for (body, a) in self.bodies.iter_mut().zip(&acceleration) {
                body.velocity += *a * (dt / 2.0);
            }
        }
        self.apply_thermostat(time);
        self.cap_kinetic_energy();
        self.recenter();
    }

    pub fn kinetic_energy(&self) -> f64 {
        self.bodies.iter().map(|b| 0.5 * b.mass * b.velocity.length_squared()).sum()
    }

    /// Kinetic + softened gravitational + wall potential energy.
    pub fn total_energy(&self) -> f64 {
        let mut potential = 0.0;
        for (i, a) in self.bodies.iter().enumerate() {
            for b in &self.bodies[i + 1..] {
                let distance = ((b.position - a.position).length_squared() + self.softening * self.softening).sqrt();
                potential -= self.gravity * a.mass * b.mass / distance;
            }
            let overshoot = (a.position.length() - self.wall_radius).max(0.0);
            potential += 0.5 * self.wall_stiffness * a.mass * overshoot * overshoot;
            potential += 0.5 * self.trap_strength * a.mass * a.position.length_squared();
        }
        self.kinetic_energy() + potential
    }

    pub fn momentum(&self) -> DVec2 {
        self.bodies.iter().map(|b| b.mass * b.velocity).sum()
    }

    /// Distance of the farthest body from the center.
    pub fn extent(&self) -> f64 {
        self.bodies.iter().map(|b| b.position.length()).fold(0.0, f64::max)
    }

    fn accelerations(&self) -> Vec<DVec2> {
        let mut result = vec![DVec2::ZERO; self.bodies.len()];
        let soft2 = self.softening * self.softening;
        for i in 0..self.bodies.len() {
            for j in i + 1..self.bodies.len() {
                let delta = self.bodies[j].position - self.bodies[i].position;
                let r2 = delta.length_squared() + soft2;
                let strength = self.gravity / (r2 * r2.sqrt());
                result[i] += delta * (strength * self.bodies[j].mass);
                result[j] -= delta * (strength * self.bodies[i].mass);
            }
            let position = self.bodies[i].position;
            result[i] -= position * self.trap_strength;
            let r = position.length();
            if r > self.wall_radius {
                result[i] -= position / r * (self.wall_stiffness * (r - self.wall_radius));
            }
        }
        result
    }

    /// Rescales velocities so total energy moves part of the way toward `target_energy`.
    fn apply_thermostat(&mut self, time: f64) {
        let Some(target) = self.target_energy else { return };
        let kinetic = self.kinetic_energy();
        if kinetic <= 1e-9 {
            return;
        }
        let correction = (target - self.total_energy()) * (self.thermostat_rate * time).min(1.0);
        let desired = (kinetic + correction).max(0.05);
        let scale = (desired / kinetic).sqrt();
        for body in &mut self.bodies {
            body.velocity *= scale;
        }
    }

    fn cap_kinetic_energy(&mut self) {
        let kinetic = self.kinetic_energy();
        if kinetic <= self.max_kinetic_energy {
            return;
        }
        let scale = (self.max_kinetic_energy / kinetic).sqrt();
        for body in &mut self.bodies {
            body.velocity *= scale;
        }
    }

    fn recenter(&mut self) {
        let total_mass: f64 = self.bodies.iter().map(|b| b.mass).sum();
        if total_mass <= 0.0 {
            return;
        }
        let center = self.bodies.iter().map(|b| b.mass * b.position).sum::<DVec2>() / total_mass;
        let drift = self.momentum() / total_mass;
        for body in &mut self.bodies {
            body.position -= center;
            body.velocity -= drift;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn figure_eight_conserves_energy_and_momentum() {
        let mut system = ThreeBodySystem::figure_eight();
        let start = system.total_energy();
        for _ in 0..(60 * 20) {
            system.advance(1.0 / 60.0);
        }
        assert!((system.total_energy() - start).abs() < start.abs() * 1e-3);
        assert!(system.momentum().length() < 1e-9);
    }

    #[test]
    fn figure_eight_returns_after_one_period() {
        let mut system = ThreeBodySystem::figure_eight();
        system.softening = 0.0;
        let start: Vec<DVec2> = system.bodies.iter().map(|b| b.position).collect();
        system.advance_with_step(6.32591398, 0.0005);
        for (body, origin) in system.bodies.iter().zip(start) {
            assert!((body.position - origin).length() < 0.01);
        }
    }

    #[test]
    fn stays_bounded_under_music_like_modulation() {
        let mut system = ThreeBodySystem::figure_eight();
        for frame in 0..(60 * 120) {
            let t = frame as f64 / 60.0;
            system.gravity = 1.0 + 0.9 * (t * 7.3).sin().abs();
            system.set_masses(&[
                1.0 + 0.8 * (t * 2.1).sin().abs(),
                1.0 + 0.8 * (t * 3.7).sin().abs(),
                1.0 + 0.8 * (t * 5.9).sin().abs(),
            ]);
            system.advance((0.45 + t.sin().abs()) / 60.0);
        }
        assert!(system.bodies.iter().all(|b| b.position.is_finite()));
        assert!(system.extent() < system.wall_radius * 2.0);
    }

    /// Mirrors the Three-Body scene: softened, gently trapped, energy target scaled with gravity and mass.
    #[test]
    fn orbit_keeps_its_size_under_music_like_modulation() {
        let mut system = ThreeBodySystem::figure_eight();
        system.softening = 0.15;
        system.trap_strength = 1.0;
        system.max_kinetic_energy = 30.0;
        let baseline = system.total_energy();
        let mut extents = Vec::new();
        for frame in 0..(60 * 90) {
            let t = frame as f64 / 60.0;
            let (low, mid, high) = ((t * 2.3).sin().abs(), (t * 1.3).sin().abs(), (t * 3.1).sin().abs());
            let masses = [1.0 + 0.8 * low, 1.0 + 0.8 * mid, 1.0 + 0.8 * high];
            system.gravity = 1.0 + 0.9 * low;
            system.set_masses(&masses);
            let mean_mass = masses.iter().sum::<f64>() / 3.0;
            system.target_energy = Some(baseline * system.gravity * mean_mass * mean_mass);
            system.advance((0.45 + 0.9 * mid) / 60.0);
            if frame > 600 {
                extents.push(system.extent());
            }
        }
        extents.sort_by(f64::total_cmp);
        assert!(extents[extents.len() / 10] > 0.4, "the orbit should not collapse into a knot");
        assert!(extents[extents.len() * 9 / 10] < system.wall_radius, "no body should be flung to the wall");
    }
}
