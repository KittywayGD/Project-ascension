//! Sauvegarde côté Bevy : photographie de la partie, F5, sauvegarde automatique,
//! sauvegarde en quittant, et application d'une partie chargée.
//!
//! Le format et l'écriture sur disque sont dans [`crate::persistence`] (Rust pur).

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::app::PlayState;
use crate::building::Buildings;
use crate::game::{GameSession, PendingSave, Screen, WorldSetup};
use crate::inventory::ItemKind;
use crate::persistence::{
    PlayerSave, SAVE_VERSION, SaveGame, SaveMeta, new_slot_id, now_unix, save_dir, write_slot,
};
use crate::player::{MoveMode, Player, PlayerInventory};
use crate::settings::Settings;
use crate::simulation::Simulation;
use crate::world::generation::WORLDGEN_VERSION;
use crate::world::{Rocks, WorldParams};

/// Plaques de fer données au départ d'une partie normale : de quoi poser extracteur,
/// four, générateur, poteau et hub, puis lancer la production.
pub const STARTER_IRON_PLATES: u32 = 50;

/// Que faire une fois la partie sauvegardée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterSave {
    Continue,
    /// Revenir à l'écran titre.
    Title,
    Quit,
}

/// Demande de sauvegarde (F5, menu pause, automatique).
#[derive(Message, Debug, Clone, Copy)]
pub struct SaveRequest {
    pub then: AfterSave,
}

/// Résultat de la dernière sauvegarde, affiché brièvement à l'écran.
#[derive(Resource, Debug, Default, Clone)]
pub struct SaveStatus {
    pub message: String,
    /// Instant (secondes réelles) jusqu'auquel le message reste affiché.
    pub until: f32,
    pub failed: bool,
    /// La partie vient d'être sauvegardée avant de quitter : pas de seconde écriture.
    exit_saved: bool,
}

/// Temps réel écoulé depuis la dernière sauvegarde automatique, en secondes.
#[derive(Resource, Debug, Default)]
struct AutosaveTimer(f32);

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SaveRequest>()
            .init_resource::<SaveStatus>()
            .init_resource::<AutosaveTimer>()
            .add_systems(
                OnEnter(Screen::InGame),
                restore_or_start.in_set(WorldSetup::Restore),
            )
            .add_systems(
                OnEnter(Screen::InGame),
                (first_save, reset_autosave).after(WorldSetup::Spawn),
            )
            .add_systems(
                Update,
                (save_key, autosave, perform_saves)
                    .chain()
                    .run_if(in_state(Screen::InGame)),
            )
            .add_systems(Last, save_on_exit.run_if(in_state(Screen::InGame)));
    }
}

/// Tout ce qu'il faut pour photographier la partie.
#[derive(SystemParam)]
pub struct SaveContext<'w, 's> {
    session: ResMut<'w, GameSession>,
    params: Res<'w, WorldParams>,
    sim: Res<'w, Simulation>,
    buildings: Res<'w, Buildings>,
    rocks: Res<'w, Rocks>,
    inventory: Res<'w, PlayerInventory>,
    player: Single<'w, 's, (&'static Transform, &'static Player)>,
}

impl SaveContext<'_, '_> {
    fn snapshot(&self) -> SaveGame {
        let (transform, player) = *self.player;
        SaveGame {
            version: SAVE_VERSION,
            meta: SaveMeta {
                name: self.session.name.clone(),
                seed: self.params.seed,
                worldgen_version: WORLDGEN_VERSION.0,
                creative: self.session.creative,
                created_unix: self.session.created_unix,
                saved_unix: now_unix(),
                ticks: self.sim.world.clock().tick(),
            },
            player: PlayerSave {
                position: transform.translation.to_array(),
                yaw: player.yaw,
                pitch: player.pitch,
                flying: player.mode == MoveMode::Fly,
                inventory: self.inventory.0.clone(),
            },
            sim: self.sim.world.clone(),
            placed: self.buildings.0.clone(),
            destroyed_rocks: self.rocks.0.destroyed().clone(),
        }
    }

    /// Sauvegarde dans l'emplacement de la session (créé au besoin). Renvoie le message
    /// à afficher, ou l'erreur.
    fn save(&mut self) -> Result<String, String> {
        let dir = save_dir();
        let slot = match &self.session.slot {
            Some(slot) => slot.clone(),
            None => {
                let slot = new_slot_id(&dir, &self.session.name);
                self.session.slot = Some(slot.clone());
                slot
            }
        };
        let save = self.snapshot();
        match write_slot(&dir, &slot, &save) {
            Ok(()) => {
                info!(
                    "Partie sauvegardée : « {} » → {}",
                    save.meta.name,
                    dir.join(format!("{slot}.ron")).display()
                );
                Ok(format!("Partie « {} » sauvegardée", save.meta.name))
            }
            Err(error) => {
                error!("Sauvegarde impossible : {}", error.message());
                Err(format!("Sauvegarde impossible : {}", error.message()))
            }
        }
    }
}

/// Partie chargée : on remet la simulation, les bâtiments, les rochers et l'inventaire.
/// Nouvelle partie normale : kit de départ.
fn restore_or_start(
    pending: Option<Res<PendingSave>>,
    session: Res<GameSession>,
    mut sim: ResMut<Simulation>,
    mut buildings: ResMut<Buildings>,
    mut rocks: ResMut<Rocks>,
    mut inventory: ResMut<PlayerInventory>,
) {
    let Some(pending) = pending else {
        if !session.creative {
            inventory.0.add(ItemKind::IronPlate, STARTER_IRON_PLATES);
        }
        return;
    };
    let save = &pending.0;
    sim.world = save.sim.clone();
    buildings.0 = save.placed.clone();
    for id in &save.destroyed_rocks {
        rocks.0.destroy(*id);
    }
    inventory.0 = save.player.inventory.clone();
    info!(
        "Partie « {} » chargée : {} bâtiments, {} convoyeurs, {} rochers détruits",
        save.meta.name,
        sim.world.buildings().count(),
        sim.world.conveyors().count(),
        save.destroyed_rocks.len()
    );
}

/// Un monde créé depuis l'écran titre est sauvegardé tout de suite : il apparaît dans la
/// liste et la sauvegarde automatique le suit.
fn first_save(
    session: Res<GameSession>,
    pending: Option<Res<PendingSave>>,
    mut requests: MessageWriter<SaveRequest>,
) {
    if session.slot.is_some() && pending.is_none() {
        requests.write(SaveRequest {
            then: AfterSave::Continue,
        });
    }
}

fn reset_autosave(mut timer: ResMut<AutosaveTimer>, mut status: ResMut<SaveStatus>) {
    timer.0 = 0.0;
    *status = SaveStatus::default();
}

fn save_key(keys: Res<ButtonInput<KeyCode>>, mut requests: MessageWriter<SaveRequest>) {
    if keys.just_pressed(KeyCode::F5) {
        requests.write(SaveRequest {
            then: AfterSave::Continue,
        });
    }
}

/// À l'intervalle choisi dans les options (2 min par défaut), pour les parties qui ont
/// un emplacement.
fn autosave(
    time: Res<Time<Real>>,
    settings: Res<Settings>,
    session: Res<GameSession>,
    state: Res<State<PlayState>>,
    mut timer: ResMut<AutosaveTimer>,
    mut requests: MessageWriter<SaveRequest>,
) {
    // Rien ne bouge en pause : inutile de réécrire le fichier.
    if session.slot.is_none() || settings.autosave_min == 0 || *state.get() == PlayState::Paused {
        return;
    }
    timer.0 += time.delta_secs();
    if timer.0 >= settings.autosave_min as f32 * 60.0 {
        timer.0 = 0.0;
        requests.write(SaveRequest {
            then: AfterSave::Continue,
        });
    }
}

fn perform_saves(
    mut requests: MessageReader<SaveRequest>,
    mut context: SaveContext,
    mut status: ResMut<SaveStatus>,
    time: Res<Time<Real>>,
    mut next: ResMut<NextState<Screen>>,
    mut exit: MessageWriter<AppExit>,
) {
    let mut then = None;
    for request in requests.read() {
        // Plusieurs demandes dans la même frame : une seule écriture, l'action la plus forte.
        then = Some(match (then, request.then) {
            (Some(AfterSave::Quit), _) | (_, AfterSave::Quit) => AfterSave::Quit,
            (Some(AfterSave::Title), _) | (_, AfterSave::Title) => AfterSave::Title,
            _ => AfterSave::Continue,
        });
    }
    let Some(then) = then else { return };
    let result = context.save();
    let failed = result.is_err();
    status.message = result.unwrap_or_else(|e| e);
    status.failed = failed;
    status.until = time.elapsed_secs() + if failed { 6.0 } else { 2.5 };
    // En cas d'échec, on reste en jeu : le joueur voit l'erreur et ne perd rien.
    if failed {
        return;
    }
    match then {
        AfterSave::Continue => {}
        AfterSave::Title => next.set(Screen::Title),
        AfterSave::Quit => {
            status.exit_saved = true;
            exit.write(AppExit::Success);
        }
    }
}

/// Dernier filet : fermeture de la fenêtre ou Maj+Échap sans passer par le menu.
fn save_on_exit(
    mut exits: MessageReader<AppExit>,
    mut context: SaveContext,
    mut status: ResMut<SaveStatus>,
) {
    if exits.read().count() == 0 || status.exit_saved {
        return;
    }
    status.exit_saved = true;
    if context.session.slot.is_some() {
        let _ = context.save();
    }
}
