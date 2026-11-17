//! Pont entre la simulation (Rust pur) et Bevy.
//!
//! Bevy appelle la simulation à pas fixe dans `FixedUpdate` ; le rendu ne fait que lire
//! son état. En pause, la simulation ne tourne pas (menu ouvert, si). En accéléré (outil de test),
//! chaque tick fixe exécute `speed` ticks de simulation.

pub mod clock;
pub mod energy;
pub mod hub;
pub mod logistics;
pub mod modules;
pub mod production;
pub mod sim_world;
pub mod tech;

use std::time::Instant;

use bevy::prelude::*;

use crate::app::PlayState;
use crate::game::{Screen, reset};
use clock::TICK_RATE_HZ;
use sim_world::SimWorld;

/// Multiplicateurs de vitesse proposés par l'outil d'accélération du temps.
pub const SPEEDS: [u32; 3] = [1, 10, 100];

#[derive(Resource, Debug)]
pub struct Simulation {
    pub world: SimWorld,
    /// Ticks de simulation exécutés par tick fixe (1 = temps réel).
    pub speed: u32,
    /// Coût moyen d'un tick de simulation, en microsecondes (moyenne glissante).
    pub tick_cost_us: f32,
}

impl Default for Simulation {
    fn default() -> Self {
        Self {
            world: SimWorld::default(),
            speed: 1,
            tick_cost_us: 0.0,
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
            .add_systems(OnExit(Screen::InGame), reset::<Simulation>)
            // Les usines tournent aussi menu ouvert ; seule la pause fige la simulation.
            .add_systems(
                FixedUpdate,
                step.run_if(in_state(PlayState::Running).or_else(in_state(PlayState::Menu))),
            );
    }
}

fn step(mut sim: ResMut<Simulation>) {
    let start = Instant::now();
    for _ in 0..sim.speed {
        sim.world.step();
    }
    // Mesure du coût CPU d'un tick (M7 : profiler la simulation à part du rendu).
    let per_tick = start.elapsed().as_secs_f32() * 1e6 / sim.speed as f32;
    sim.tick_cost_us = if sim.tick_cost_us == 0.0 {
        per_tick
    } else {
        sim.tick_cost_us * 0.95 + per_tick * 0.05
    };
}
