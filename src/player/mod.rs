//! Joueur : mode marche (gravité, saut) et mode vol, bascule avec V.
//!
//! Pas de moteur physique : le sol est la hauteur exacte du maillage du terrain
//! (`HeightField::height_at`). Les touches sont des `KeyCode` physiques :
//! W/A/S/D en QWERTY correspondent automatiquement à Z/Q/S/D sur un clavier AZERTY.

use std::f32::consts::FRAC_PI_2;

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;

use crate::app::PlayState;
use crate::building::Buildings;
use crate::collision::{collide, highest_top};
use crate::game::{PendingSave, Screen, WorldSetup, reset};
use crate::inventory::Inventory;
use crate::settings::Settings;
use crate::world::generation::MAX_WADING_DEPTH_M;
use crate::world::sky::camera_rendering;
use crate::world::{Rocks, Terrain};

/// Hauteur des yeux au-dessus du sol, en mètres.
pub const EYE_HEIGHT: f32 = 1.7;
/// Gravité « de jeu », plus forte que 9,81 m/s² pour des sauts moins flottants.
const GRAVITY: f32 = 18.0;
/// Vitesse verticale au départ d'un saut (≈ 1 m de hauteur avec la gravité ci-dessus).
const JUMP_SPEED: f32 = 6.0;
/// Dénivelé maximal suivi sans décoller lorsqu'on descend une pente, en mètres.
const STEP_DOWN: f32 = 0.6;
/// Hauteur d'obstacle (rocher) que l'on enjambe sans sauter, en mètres.
const STEP_UP: f32 = 0.45;
/// Rayon du corps du joueur pour les collisions, en mètres.
pub const PLAYER_RADIUS: f32 = 0.3;
/// Distance minimale entre la caméra et le sol (ou la mer) en vol.
const FLY_CLEARANCE: f32 = 0.5;
/// Pas de temps maximal pris en compte, pour éviter les sauts après un ralentissement.
const MAX_FRAME_DT: f32 = 0.1;
/// Limite de tangage, juste sous la verticale pour éviter le retournement.
const PITCH_LIMIT: f32 = FRAC_PI_2 - 0.01;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MoveMode {
    #[default]
    Walk,
    Fly,
}

#[derive(Component, Debug)]
pub struct Player {
    /// Lacet (rotation autour de Y), en radians.
    pub yaw: f32,
    /// Tangage (rotation autour de X local), en radians.
    pub pitch: f32,
    pub mode: MoveMode,
    /// Vitesse verticale en mode marche, en m/s.
    pub vertical_speed: f32,
    /// Au sol (le saut est possible).
    pub grounded: bool,
    /// Vitesse réelle de la dernière frame (déplacement effectif / durée), en m/s.
    pub velocity: Vec3,
    pub walk_speed: f32,
    pub run_speed: f32,
    pub fly_speed: f32,
    /// Multiplicateur appliqué en vol en maintenant Maj.
    pub fly_sprint_multiplier: f32,
    /// Radians par pixel de déplacement de la souris.
    pub sensitivity: f32,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            mode: MoveMode::Walk,
            vertical_speed: 0.0,
            grounded: false,
            velocity: Vec3::ZERO,
            walk_speed: 4.5,
            run_speed: 8.0,
            fly_speed: 10.0,
            fly_sprint_multiplier: 4.0,
            sensitivity: 0.002,
        }
    }
}

/// Nombre de cases de l'inventaire du joueur (3 rangées de 9).
pub const INVENTORY_SLOTS: usize = 27;

/// Inventaire du joueur (données de jeu, à sauvegarder en V0.1).
#[derive(Resource, Debug)]
pub struct PlayerInventory(pub Inventory);

impl Default for PlayerInventory {
    fn default() -> Self {
        Self(Inventory::new(INVENTORY_SLOTS))
    }
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerInventory>()
            .add_systems(
                OnEnter(Screen::InGame),
                spawn_player.in_set(WorldSetup::Spawn),
            )
            .add_systems(OnExit(Screen::InGame), reset::<PlayerInventory>)
            .add_systems(
                Update,
                (toggle_mode, look_around, move_player)
                    .chain()
                    .run_if(in_state(PlayState::Running)),
            );
    }
}

fn spawn_player(mut commands: Commands, terrain: Res<Terrain>, pending: Option<Res<PendingSave>>) {
    // Partie chargée : position, regard et mode sauvegardés.
    if let Some(pending) = pending {
        let saved = &pending.0.player;
        let player = Player {
            yaw: saved.yaw,
            pitch: saved.pitch,
            mode: if saved.flying {
                MoveMode::Fly
            } else {
                MoveMode::Walk
            },
            ..default()
        };
        commands.spawn((
            Name::new("Joueur"),
            Camera3d::default(),
            Transform::from_translation(Vec3::from_array(saved.position)).with_rotation(
                Quat::from_euler(EulerRot::YXZ, player.yaw, player.pitch, 0.0),
            ),
            camera_rendering(),
            player,
        ));
        return;
    }

    let field = &terrain.0;
    let (x, z) = field.find_spawn().unwrap_or_else(|| {
        warn!("Aucune terre émergée trouvée : apparition au centre");
        (0.0, 0.0)
    });
    let y = field.height_at(x, z).max(0.0) + EYE_HEIGHT;
    info!("Apparition du joueur en ({x:.0}, {y:.1}, {z:.0})");

    commands.spawn((
        Name::new("Joueur"),
        Camera3d::default(),
        Transform::from_xyz(x, y, z),
        camera_rendering(),
        Player::default(),
    ));
}

fn toggle_mode(keys: Res<ButtonInput<KeyCode>>, mut player: Single<&mut Player>) {
    if keys.just_pressed(KeyCode::KeyV) {
        player.mode = match player.mode {
            MoveMode::Walk => MoveMode::Fly,
            MoveMode::Fly => MoveMode::Walk,
        };
        // En repassant en marche, on retombe depuis l'altitude actuelle.
        player.vertical_speed = 0.0;
        player.grounded = false;
        info!("Mode {:?}", player.mode);
    }
}

fn look_around(
    mouse: Res<AccumulatedMouseMotion>,
    settings: Res<Settings>,
    player: Single<(&mut Transform, &mut Player)>,
) {
    let delta = mouse.delta;
    if delta == Vec2::ZERO {
        return;
    }
    let (mut transform, mut player) = player.into_inner();
    // Pas de multiplication par le temps : un déplacement de souris est déjà une distance.
    let sensitivity = player.sensitivity * settings.mouse_sensitivity as f32 / 100.0;
    let vertical = if settings.invert_y { -1.0 } else { 1.0 };
    player.yaw -= delta.x * sensitivity;
    player.pitch =
        (player.pitch - vertical * delta.y * sensitivity).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    transform.rotation = Quat::from_euler(EulerRot::YXZ, player.yaw, player.pitch, 0.0);
}

fn move_player(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    terrain: Res<Terrain>,
    rocks: Res<Rocks>,
    buildings: Res<Buildings>,
    player: Single<(&mut Transform, &mut Player)>,
) {
    let (mut transform, mut player) = player.into_inner();
    let field = &terrain.0;
    let obstacles = || rocks.0.obstacles().chain(buildings.0.obstacles());
    let dt = time.delta_secs().min(MAX_FRAME_DT);
    let sprint = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let forward = axis(&keys, KeyCode::KeyW, KeyCode::KeyS);
    let right = axis(&keys, KeyCode::KeyD, KeyCode::KeyA);
    let half = field.config().half_size();
    let start = transform.translation;
    let mut pos = start;

    match player.mode {
        MoveMode::Fly => {
            let input = MoveInput {
                forward,
                right,
                up: axis(&keys, KeyCode::Space, KeyCode::ControlLeft),
            };
            let multiplier = if sprint {
                player.fly_sprint_multiplier
            } else {
                1.0
            };
            pos += wish_direction(input, player.yaw) * player.fly_speed * multiplier * dt;
            pos = clamp_to_map(pos, half);
            // En vol, on glisse par-dessus les rochers plutôt que de les traverser.
            let ground = field.height_at(pos.x, pos.z).max(0.0);
            let below = highest_top(pos.x, pos.z, PLAYER_RADIUS, obstacles());
            let floor = below.map_or(ground, |top| top.max(ground)) + FLY_CLEARANCE;
            pos.y = pos.y.max(floor);
        }
        MoveMode::Walk => {
            let input = MoveInput {
                forward,
                right,
                up: 0.0,
            };
            let speed = if sprint {
                player.run_speed
            } else {
                player.walk_speed
            };
            let step = wish_direction(input, player.yaw) * speed * dt;
            let target = clamp_to_map(pos + step, half);
            let here = field.height_at(pos.x, pos.z);
            let there = field.height_at(target.x, target.z);
            if can_walk_to(here, there) {
                pos.x = target.x;
                pos.z = target.z;
            }

            // Rochers et bâtiments : repoussé par les hauts, porté par les bas.
            let feet = pos.y - EYE_HEIGHT;
            let hit = collide(pos.x, pos.z, feet, PLAYER_RADIUS, STEP_UP, obstacles());
            pos = clamp_to_map(Vec3::new(hit.x, pos.y, hit.z), half);
            let terrain_ground = field.height_at(pos.x, pos.z);
            let ground = hit
                .support
                .map_or(terrain_ground, |top| top.max(terrain_ground));
            let ground_eye = ground + EYE_HEIGHT;
            let jump = keys.just_pressed(KeyCode::Space);
            let next = vertical_step(
                Vertical {
                    y: pos.y,
                    speed: player.vertical_speed,
                    grounded: player.grounded,
                },
                jump,
                dt,
                ground_eye,
            );
            pos.y = next.y;
            player.vertical_speed = next.speed;
            player.grounded = next.grounded;
        }
    }

    transform.translation = pos;
    player.velocity = if dt > 0.0 {
        (pos - start) / dt
    } else {
        Vec3::ZERO
    };
}

fn axis(keys: &ButtonInput<KeyCode>, positive: KeyCode, negative: KeyCode) -> f32 {
    f32::from(keys.pressed(positive)) - f32::from(keys.pressed(negative))
}

/// Les bords de la carte sont une limite dure (le terrain y plonge déjà sous la mer).
fn clamp_to_map(pos: Vec3, half: f32) -> Vec3 {
    Vec3::new(pos.x.clamp(-half, half), pos.y, pos.z.clamp(-half, half))
}

/// À pied, on refuse d'entrer en eau profonde, sauf pour en sortir (vers moins profond).
fn can_walk_to(here: f32, there: f32) -> bool {
    there >= -MAX_WADING_DEPTH_M || there >= here
}

/// Entrées de déplacement, chaque axe dans [-1, 1].
#[derive(Debug, Clone, Copy, Default)]
struct MoveInput {
    forward: f32,
    right: f32,
    up: f32,
}

/// Direction de déplacement normalisée dans le repère monde.
///
/// Avant/droite suivent uniquement le lacet (on reste à l'horizontale même en
/// regardant vers le bas) ; haut/bas suivent l'axe Y du monde.
fn wish_direction(input: MoveInput, yaw: f32) -> Vec3 {
    let horizontal = Quat::from_rotation_y(yaw) * Vec3::new(input.right, 0.0, -input.forward);
    (horizontal + Vec3::Y * input.up).normalize_or_zero()
}

/// État vertical du joueur en mode marche.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Vertical {
    y: f32,
    speed: f32,
    grounded: bool,
}

/// Un pas de gravité. `ground_eye` est la hauteur des yeux si l'on se tenait au sol ici.
///
/// En descente, on reste collé au sol tant que le dénivelé de la frame est sous
/// `STEP_DOWN` ; au-delà (falaise), on tombe.
fn vertical_step(state: Vertical, jump: bool, dt: f32, ground_eye: f32) -> Vertical {
    let mut speed = state.speed;
    if state.grounded && jump {
        speed = JUMP_SPEED;
    }
    speed -= GRAVITY * dt;
    let y = state.y + speed * dt;

    let snap_down = state.grounded && speed <= 0.0 && y - ground_eye <= STEP_DOWN;
    if y <= ground_eye || snap_down {
        Vertical {
            y: ground_eye,
            speed: 0.0,
            grounded: true,
        }
    } else {
        Vertical {
            y,
            speed,
            grounded: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-5;
    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn forward_without_rotation_goes_towards_negative_z() {
        let input = MoveInput {
            forward: 1.0,
            ..default()
        };
        assert!(wish_direction(input, 0.0).abs_diff_eq(Vec3::NEG_Z, EPS));
    }

    #[test]
    fn forward_follows_yaw() {
        // Un quart de tour à gauche : l'avant pointe vers -X.
        let input = MoveInput {
            forward: 1.0,
            ..default()
        };
        assert!(wish_direction(input, FRAC_PI_2).abs_diff_eq(Vec3::NEG_X, EPS));
    }

    #[test]
    fn diagonal_is_not_faster() {
        let input = MoveInput {
            forward: 1.0,
            right: 1.0,
            up: 1.0,
        };
        assert!((wish_direction(input, 0.3).length() - 1.0).abs() < EPS);
    }

    #[test]
    fn no_input_means_no_movement() {
        assert_eq!(wish_direction(MoveInput::default(), 1.0), Vec3::ZERO);
    }

    #[test]
    fn falls_and_lands_on_the_ground() {
        let mut state = Vertical {
            y: 20.0,
            speed: 0.0,
            grounded: false,
        };
        for _ in 0..600 {
            state = vertical_step(state, false, DT, 5.0);
        }
        assert_eq!(
            state,
            Vertical {
                y: 5.0,
                speed: 0.0,
                grounded: true
            }
        );
    }

    #[test]
    fn jump_leaves_the_ground_and_comes_back() {
        let ground = Vertical {
            y: 1.7,
            speed: 0.0,
            grounded: true,
        };
        let mut state = vertical_step(ground, true, DT, 1.7);
        assert!(!state.grounded && state.y > 1.7);
        let mut apex = state.y;
        for _ in 0..120 {
            state = vertical_step(state, false, DT, 1.7);
            apex = apex.max(state.y);
        }
        assert!(state.grounded);
        // Apex ≈ v² / 2g = 1 m au-dessus des yeux.
        assert!((apex - 1.7 - 1.0).abs() < 0.1, "apex {apex}");
    }

    #[test]
    fn cannot_jump_in_mid_air() {
        let airborne = Vertical {
            y: 10.0,
            speed: 0.0,
            grounded: false,
        };
        let next = vertical_step(airborne, true, DT, 1.7);
        assert!(next.speed < 0.0);
    }

    #[test]
    fn stays_on_the_ground_when_walking_downhill() {
        let walking = Vertical {
            y: 10.0,
            speed: 0.0,
            grounded: true,
        };
        // Le sol est 10 cm plus bas que la frame précédente.
        let next = vertical_step(walking, false, DT, 9.9);
        assert!(next.grounded);
        assert_eq!(next.y, 9.9);
    }

    #[test]
    fn falls_off_a_cliff() {
        let walking = Vertical {
            y: 10.0,
            speed: 0.0,
            grounded: true,
        };
        let next = vertical_step(walking, false, DT, 5.0);
        assert!(!next.grounded);
    }

    #[test]
    fn deep_water_blocks_walking_except_towards_the_shore() {
        assert!(can_walk_to(3.0, -0.5)); // on peut patauger
        assert!(!can_walk_to(-1.0, -2.0)); // pas plus loin vers le large
        assert!(can_walk_to(-5.0, -4.0)); // on peut toujours remonter vers la plage
    }

    #[test]
    fn map_edges_are_a_hard_limit() {
        let clamped = clamp_to_map(Vec3::new(300.0, 4.0, -999.0), 256.0);
        assert_eq!(clamped, Vec3::new(256.0, 4.0, -256.0));
    }
}
