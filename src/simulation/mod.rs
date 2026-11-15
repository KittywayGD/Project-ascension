//! Pont entre la simulation (Rust pur) et Bevy.
//!
//! Bevy appelle la simulation à pas fixe dans `FixedUpdate` ; le rendu ne fait que lire
//! son état. En pause, la simulation ne tourne pas (menu ouvert, si). En accéléré (outil de test),
//! chaque tick fixe exécute `speed` ticks de simulation.

pub mod clock;
pub mod energy;
pub mod logistics;
pub mod production;
pub mod sim_world;

use bevy::prelude::*;

use crate::app::PlayState;
use clock::TICK_RATE_HZ;
use sim_world::SimWorld;

/// Multiplicateurs de vitesse proposés par l'outil d'accélération du temps.
pub const SPEEDS: [u32; 3] = [1, 10, 100];

#[derive(Resource, Debug)]
pub struct Simulation {
    pub world: SimWorld,
    /// Ticks de simulation exécutés par tick fixe (1 = temps réel).
    pub speed: u32,
}

impl Default for Simulation {
    fn default() -> Self {
        Self {
            world: SimWorld::default(),
            speed: 1,
        }
    }
}

impl Simulation {
    /// Passe à la vitesse suivante (×1 → ×10 → ×100 → ×1).
    pub fn cycle_speed(&mut self) {
        let index = SPEEDS.iter().position(|&s| s == self.speed).unwrap_or(0);
        self.speed = SPEEDS[(index + 1) % SPEEDS.len()];
    }
}

pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(f64::from(TICK_RATE_HZ)))
            .init_resource::<Simulation>()
            // Les usines tournent aussi menu ouvert ; seule la pause fige la simulation.
            .add_systems(FixedUpdate, step.run_if(not(in_state(PlayState::Paused))));
    }
}

fn step(mut sim: ResMut<Simulation>) {
    for _ in 0..sim.speed {
        sim.world.step();
    }
}
