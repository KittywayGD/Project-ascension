//! Project Ascension — sandbox industriel 3D.
//!
//! Conventions (voir TODO §1) : 1 unité = 1 mètre, axe Y vers le haut,
//! repère main droite de Bevy (la caméra regarde vers -Z par défaut).

mod app;
mod building;
mod collision;
mod config;
mod debug;
mod interaction;
mod inventory;
mod player;
mod simulation;
mod ui;
mod world;

use bevy::log::{Level, LogPlugin};
use bevy::prelude::*;

use config::LaunchOptions;
use world::WorldParams;
use world::generation::WorldGenConfig;

fn main() -> AppExit {
    let options = match LaunchOptions::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("Erreur : {message}\nUsage : cargo dev -- [--seed <entier>]");
            return AppExit::error();
        }
    };

    App::new()
        .insert_resource(WorldParams {
            seed: options.seed_or_random(),
            config: WorldGenConfig::default(),
        })
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Project Ascension".into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(LogPlugin {
                    level: Level::INFO,
                    // On coupe le bruit des couches graphiques, on garde nos logs `info`.
                    filter: "info,wgpu=error,naga=warn".into(),
                    ..default()
                }),
        )
        .add_plugins((
            app::AppStatePlugin,
            world::WorldPlugin,
            player::PlayerPlugin,
            simulation::SimulationPlugin,
            interaction::InteractionPlugin,
            building::BuildingPlugin,
            ui::MenuPlugin,
            debug::DebugPlugin,
        ))
        .run()
}
