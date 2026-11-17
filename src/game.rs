//! Écrans du jeu (titre / en jeu), session en cours et mise en place d'un monde.
//!
//! Le monde n'existe qu'à l'écran « en jeu » : il est généré en entrant (nouvelle partie
//! ou chargement), puis tout ce qui l'affiche est supprimé en revenant au titre.
//! L'ordre de mise en place est fixé par [`WorldSetup`].

use bevy::prelude::*;

use crate::persistence::SaveGame;
use crate::world::WorldParams;
use crate::world::generation::{WorldGenConfig, WorldSeed};

/// Écran principal.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Screen {
    /// Démarrage : les ressources globales (matériaux, police) se chargent pendant
    /// `Startup`, puis on passe au titre ou directement en jeu.
    #[default]
    Boot,
    /// Écran titre : liste des mondes, nouveau monde.
    Title,
    /// Un monde est chargé.
    InGame,
}

/// Étapes de la mise en place d'un monde, à l'entrée de [`Screen::InGame`], dans l'ordre.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorldSetup {
    /// Relief, rochers, gisements (depuis la seed).
    Generate,
    /// État sauvegardé (ou scène de démo) appliqué aux données.
    Restore,
    /// Entités d'affichage : terrain, joueur, interface…
    Spawn,
}

/// Entité racine conservée en quittant une partie (soleil, atmosphère).
#[derive(Component, Debug, Default)]
pub struct Persistent;

/// La partie en cours.
#[derive(Resource, Debug, Clone)]
pub struct GameSession {
    /// Emplacement de sauvegarde ; `None` tant que la partie n'a jamais été sauvegardée.
    pub slot: Option<String>,
    pub name: String,
    pub creative: bool,
    pub created_unix: u64,
}

/// Partie chargée, appliquée pendant [`WorldSetup::Restore`] puis retirée.
#[derive(Resource)]
pub struct PendingSave(pub Box<SaveGame>);

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Screen>()
            .configure_sets(
                OnEnter(Screen::InGame),
                (WorldSetup::Generate, WorldSetup::Restore, WorldSetup::Spawn).chain(),
            )
            .add_systems(
                OnEnter(Screen::InGame),
                forget_pending_save.after(WorldSetup::Spawn),
            )
            .add_systems(PostStartup, leave_boot)
            .add_systems(OnExit(Screen::InGame), despawn_world);
    }
}

/// Fin du démarrage : en jeu si une partie a été préparée (options de lancement), sinon titre.
fn leave_boot(session: Option<Res<GameSession>>, mut next: ResMut<NextState<Screen>>) {
    next.set(if session.is_some() {
        Screen::InGame
    } else {
        Screen::Title
    });
}

/// Prépare une nouvelle partie ; il reste à passer à [`Screen::InGame`].
pub fn start_new_game(
    commands: &mut Commands,
    slot: Option<String>,
    name: String,
    seed: WorldSeed,
    creative: bool,
) {
    commands.insert_resource(WorldParams {
        seed,
        config: WorldGenConfig::default(),
    });
    commands.insert_resource(GameSession {
        slot,
        name,
        creative,
        created_unix: crate::persistence::now_unix(),
    });
}

/// Prépare le chargement d'une partie sauvegardée dans l'emplacement `slot`.
pub fn start_loaded_game(commands: &mut Commands, slot: String, save: SaveGame) {
    commands.queue(move |world: &mut World| {
        insert_loaded_game(world, slot, &save);
        world.insert_resource(PendingSave(Box::new(save)));
    });
}

/// Paramètres du monde et session d'une partie chargée (sans la partie elle-même).
pub fn insert_loaded_game(world: &mut World, slot: String, save: &SaveGame) {
    world.insert_resource(WorldParams {
        seed: save.meta.seed,
        config: WorldGenConfig::default(),
    });
    world.insert_resource(GameSession {
        slot: Some(slot),
        name: save.meta.name.clone(),
        creative: save.meta.creative,
        created_unix: save.meta.created_unix,
    });
}

fn forget_pending_save(mut commands: Commands) {
    commands.remove_resource::<PendingSave>();
}

/// Remet une ressource à zéro (à brancher sur `OnExit(Screen::InGame)`).
pub fn reset<R: Resource + Default>(mut commands: Commands) {
    commands.insert_resource(R::default());
}

/// Supprime tout ce qui affiche le monde : entités racines visibles, sauf [`Persistent`].
fn despawn_world(
    roots: Query<
        Entity,
        (
            Without<ChildOf>,
            Without<Persistent>,
            Or<(With<Visibility>, With<Node>, With<Camera>)>,
        ),
    >,
    mut commands: Commands,
) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<GameSession>();
    commands.remove_resource::<WorldParams>();
}
