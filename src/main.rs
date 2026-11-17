//! Project Ascension — sandbox industriel 3D.
//!
//! Conventions (voir TODO §1) : 1 unité = 1 mètre, axe Y vers le haut,
//! repère main droite de Bevy (la caméra regarde vers -Z par défaut).

mod app;
mod building;
mod collision;
mod config;
mod debug;
mod game;
mod interaction;
mod inventory;
mod persistence;
mod player;
mod save;
mod scene;
mod settings;
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
            eprintln!("Erreur : {message}\n{}", config::USAGE);
            return AppExit::error();
        }
    };

    let mut app = App::new();
    app.insert_resource(building::DemoRequested(options.demo))
        .insert_resource(debug::capture::CaptureRequest(options.screenshot.clone()))
        .insert_resource(debug::capture::TitleCaptureRequest(
            options.title_screenshot.clone(),
        ))
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
            game::GamePlugin,
            app::AppStatePlugin,
            world::WorldPlugin,
            player::PlayerPlugin,
            simulation::SimulationPlugin,
            interaction::InteractionPlugin,
            building::BuildingPlugin,
            ui::MenuPlugin,
            save::SavePlugin,
            settings::SettingsPlugin,
            ui::ScreensPlugin,
            debug::DebugPlugin,
        ));

    // Options de lancement : on saute l'écran titre et on joue directement.
    if let Some(slot) = &options.load {
        match persistence::read_slot(&persistence::save_dir(), slot) {
            Ok(save) => {
                app.insert_resource(game::PendingSave(Box::new(save.clone())));
                game::insert_loaded_game(app.world_mut(), slot.clone(), &save);
            }
            Err(error) => {
                eprintln!(
                    "Erreur : impossible de charger « {slot} » : {}",
                    error.message()
                );
                return AppExit::error();
            }
        }
    } else if options.skips_title() {
        let seed = options.seed_or_random();
        let name = options.name.clone().unwrap_or_else(|| {
            if options.demo {
                "Démo".to_string()
            } else {
                format!("Seed {}", seed.0)
            }
        });
        // Nommée : la partie a un emplacement et se sauvegarde ; sinon, F5 en crée un.
        let slot = options
            .name
            .as_ref()
            .map(|name| persistence::new_slot_id(&persistence::save_dir(), name));
        app.insert_resource(WorldParams {
            seed,
            config: WorldGenConfig::default(),
        })
        .insert_resource(game::GameSession {
            slot,
            name,
            creative: options.demo,
            created_unix: persistence::now_unix(),
        });
    }
    app.run()
}
