//! Construction : outil choisi dans le menu (B), fantôme, rotation (R), pose (clic gauche),
//! démontage (clic droit). Affiche aussi les bâtiments, leurs voyants, les câbles et les
//! convoyeurs avec les objets qu'ils transportent.
//!
//! Les règles sont dans `placement.rs` (Rust pur) ; ce module ne fait qu'afficher et
//! relier les entrées du joueur à la simulation. Pose gratuite en V0 (mode créatif).

pub mod placement;

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use crate::app::PlayState;
use crate::interaction::{Aim, Target, update_target};
use crate::inventory::ItemKind;
use crate::player::{PLAYER_RADIUS, Player};
use crate::simulation::Simulation;
use crate::simulation::energy::{GENERATOR_POWER_W, POLE_RANGE_M};
use crate::simulation::logistics::{
    CONVEYOR_MAX_LENGTH_M, CONVEYOR_SPEED_MPS, conveyor_max_rate_per_min,
};
use crate::simulation::production::{
    EXTRACTOR_POWER_W, FURNACE_POWER_W, MachineState, extractor_rate_per_min,
};
use crate::simulation::sim_world::{BuildingId, BuildingKind, Machine, STORAGE_SLOTS};
use crate::world::{Deposits, Rocks, Terrain};
use placement::{
    ConveyorPreview, PlacedBuilding, PlacedBuildings, PlacementContext, PlacementPreview, dims,
    preview, preview_conveyor,
};

/// Outil de construction choisi dans le menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildTool {
    Building(BuildingKind),
    Conveyor,
}

impl BuildTool {
    /// Ordre d'affichage dans le menu de construction.
    pub const ALL: [BuildTool; 6] = [
        BuildTool::Building(BuildingKind::Extractor),
        BuildTool::Building(BuildingKind::Generator),
        BuildTool::Building(BuildingKind::Furnace),
        BuildTool::Building(BuildingKind::Storage),
        BuildTool::Building(BuildingKind::Pole),
        BuildTool::Conveyor,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BuildTool::Building(kind) => kind.name(),
            BuildTool::Conveyor => "Convoyeur",
        }
    }

    /// Ligne de caractéristiques (énergie, débit…) pour le menu.
    pub fn stats(self) -> String {
        match self {
            BuildTool::Building(BuildingKind::Extractor) => format!(
                "{} kW · 15 à 60 /min selon la pureté",
                EXTRACTOR_POWER_W / 1000
            ),
            BuildTool::Building(BuildingKind::Generator) => {
                format!("+{} kW", GENERATOR_POWER_W / 1000)
            }
            BuildTool::Building(BuildingKind::Furnace) => {
                format!("{} kW · 1 plaque / 2 s", FURNACE_POWER_W / 1000)
            }
            BuildTool::Building(BuildingKind::Storage) => format!("{STORAGE_SLOTS} cases"),
            BuildTool::Building(BuildingKind::Pole) => format!("portée {POLE_RANGE_M:.0} m"),
            BuildTool::Conveyor => format!(
                "{}/min max · {CONVEYOR_MAX_LENGTH_M:.0} m max",
                conveyor_max_rate_per_min()
            ),
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            BuildTool::Building(BuildingKind::Extractor) => {
                "Se pose sur un gisement et en extrait le minerai, sans fin."
            }
            BuildTool::Building(BuildingKind::Generator) => {
                "Alimente les machines reliées par des poteaux."
            }
            BuildTool::Building(BuildingKind::Furnace) => {
                "Fond le minerai en plaques. Entrée à l'arrière, sortie à droite."
            }
            BuildTool::Building(BuildingKind::Storage) => {
                "Stocke tout ce qu'on lui apporte. Entrée à l'arrière, sortie à l'avant."
            }
            BuildTool::Building(BuildingKind::Pole) => {
                "Relie au réseau tout bâtiment à portée, et les autres poteaux."
            }
            BuildTool::Conveyor => {
                "Relie une sortie (bloc jaune) à une entrée (bloc bleu). Deux clics."
            }
        }
    }
}

#[derive(Resource, Debug)]
pub struct BuildMode {
    pub active: bool,
    pub tool: BuildTool,
    /// Orientation du prochain bâtiment, en quarts de tour.
    pub rotation: u8,
    /// Convoyeur en cours de pose : bâtiment de départ déjà choisi.
    pub conveyor_from: Option<BuildingId>,
}

impl Default for BuildMode {
    fn default() -> Self {
        Self {
            active: false,
            tool: BuildTool::Building(BuildingKind::Extractor),
            rotation: 0,
            conveyor_from: None,
        }
    }
}

impl BuildMode {
    /// Passe en mode pose avec l'outil choisi.
    pub fn select(&mut self, tool: BuildTool) {
        self.active = true;
        self.tool = tool;
        self.conveyor_from = None;
    }

    pub fn stop(&mut self) {
        self.active = false;
        self.conveyor_from = None;
    }
}

/// Bâtiments posés (données de jeu) et leurs entités d'affichage.
#[derive(Resource, Debug, Default)]
pub struct Buildings(pub PlacedBuildings);

#[derive(Resource, Debug, Default)]
struct BuildingEntities(HashMap<BuildingId, Entity>);

/// Aperçu de pose courant (pour le fantôme et le bandeau d'aide).
#[derive(Resource, Debug, Default)]
pub struct Preview {
    pub building: Option<PlacementPreview>,
    pub conveyor: Option<ConveyorPreview>,
}

/// Rôle d'une pièce de modèle : certaines changent d'aspect selon l'état de la machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartRole {
    Plain,
    /// Voyant d'état.
    Light,
    /// Gueule du four, qui rougeoie quand il fond.
    Mouth,
    /// Entrée (bloc bleu), où arrive un convoyeur.
    InputPort,
    /// Sortie (bloc jaune), d'où part un convoyeur.
    OutputPort,
}

struct Part {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    offset: Vec3,
    role: PartRole,
}

#[derive(Resource)]
struct BuildingAssets {
    unit_cube: Handle<Mesh>,
    cable: Handle<Mesh>,
    belt: Handle<Mesh>,
    item: Handle<Mesh>,
    parts: HashMap<BuildingKind, Vec<Part>>,
    foundation_material: Handle<StandardMaterial>,
    cable_material: Handle<StandardMaterial>,
    belt_material: Handle<StandardMaterial>,
    item_materials: HashMap<ItemKind, Handle<StandardMaterial>>,
    light_active: Handle<StandardMaterial>,
    light_waiting: Handle<StandardMaterial>,
    light_no_power: Handle<StandardMaterial>,
    light_off: Handle<StandardMaterial>,
    mouth_idle: Handle<StandardMaterial>,
    mouth_hot: Handle<StandardMaterial>,
    ghost_valid: Handle<StandardMaterial>,
    ghost_invalid: Handle<StandardMaterial>,
    /// Ports du fantôme : toujours bleu (entrée) et jaune (sortie), quelle que soit la validité.
    ghost_input: Handle<StandardMaterial>,
    ghost_output: Handle<StandardMaterial>,
}

#[derive(Component)]
struct Ghost(BuildingKind);

#[derive(Component)]
struct ConveyorGhost;

#[derive(Component)]
struct GhostPart;

/// Port d'un fantôme : garde sa couleur bleue ou jaune.
#[derive(Component)]
struct GhostPort;

#[derive(Component)]
struct Foundation;

#[derive(Component)]
struct StatusLight(BuildingId);

#[derive(Component)]
struct FurnaceMouth(BuildingId);

#[derive(Component)]
struct Cable;

/// Affichage d'un convoyeur : la racine est orientée de la sortie vers l'entrée.
#[derive(Component)]
struct ConveyorVisual {
    id: BuildingId,
    length: f32,
    /// Cubes recyclés pour afficher les objets transportés.
    pool: Vec<Entity>,
}

#[derive(Component)]
struct BuildHud;

pub struct BuildingPlugin;

impl Plugin for BuildingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildMode>()
            .init_resource::<Buildings>()
            .init_resource::<BuildingEntities>()
            .init_resource::<Preview>()
            .add_systems(Startup, (setup_assets, spawn_ghosts, spawn_hud).chain())
            .add_systems(
                Update,
                (rotate_key, update_preview, update_ghosts, place, remove)
                    .chain()
                    .after(update_target)
                    .run_if(in_state(PlayState::Running)),
            )
            .add_systems(
                Update,
                (
                    update_status_parts,
                    update_cables,
                    sync_conveyors,
                    update_conveyor_items,
                    update_hud,
                )
                    .chain()
                    .after(remove),
            );
    }
}

fn setup_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let plain = |materials: &mut Assets<StandardMaterial>, color: Color| {
        materials.add(StandardMaterial {
            base_color: color,
            perceptual_roughness: 0.6,
            ..default()
        })
    };
    let glow = |materials: &mut Assets<StandardMaterial>, color: Color, emissive: LinearRgba| {
        materials.add(StandardMaterial {
            base_color: color,
            emissive,
            ..default()
        })
    };
    let ghost = |materials: &mut Assets<StandardMaterial>, color: Color| {
        materials.add(StandardMaterial {
            base_color: color,
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        })
    };

    let orange = plain(&mut materials, Color::srgb(0.80, 0.45, 0.15));
    let steel = plain(&mut materials, Color::srgb(0.35, 0.35, 0.38));
    let output = plain(&mut materials, Color::srgb(0.95, 0.80, 0.20));
    let input = plain(&mut materials, Color::srgb(0.20, 0.50, 0.95));
    let blue_gray = plain(&mut materials, Color::srgb(0.30, 0.36, 0.45));
    let copper = plain(&mut materials, Color::srgb(0.75, 0.45, 0.20));
    let brick = plain(&mut materials, Color::srgb(0.55, 0.30, 0.22));
    let wood = plain(&mut materials, Color::srgb(0.45, 0.32, 0.20));
    let crate_wood = plain(&mut materials, Color::srgb(0.62, 0.47, 0.28));
    let light_active = glow(
        &mut materials,
        Color::srgb(0.2, 0.9, 0.3),
        LinearRgba::rgb(0.5, 4.0, 0.8),
    );
    let light_waiting = glow(
        &mut materials,
        Color::srgb(1.0, 0.6, 0.1),
        LinearRgba::rgb(4.0, 1.8, 0.2),
    );
    let light_no_power = glow(
        &mut materials,
        Color::srgb(1.0, 0.15, 0.1),
        LinearRgba::rgb(4.0, 0.3, 0.2),
    );
    let light_off = plain(&mut materials, Color::srgb(0.25, 0.25, 0.25));
    let mouth_idle = plain(&mut materials, Color::srgb(0.08, 0.06, 0.05));
    let mouth_hot = glow(
        &mut materials,
        Color::srgb(1.0, 0.5, 0.1),
        LinearRgba::rgb(6.0, 2.0, 0.3),
    );

    let part = |mesh: Handle<Mesh>, material: &Handle<StandardMaterial>, offset: Vec3, role| Part {
        mesh,
        material: material.clone(),
        offset,
        role,
    };
    let light_mesh = meshes.add(Sphere::new(0.2));
    // Ports : bloc jaune = sortie, bloc bleu = entrée (même position que dans `placement.rs`).
    let port_z = meshes.add(Cuboid::new(0.8, 0.8, 0.3));
    let port_x = meshes.add(Cuboid::new(0.3, 0.8, 0.8));
    let mut parts = HashMap::new();
    parts.insert(
        BuildingKind::Extractor,
        vec![
            part(
                meshes.add(Cuboid::new(2.4, 1.4, 2.4)),
                &orange,
                Vec3::new(0.0, 0.7, 0.0),
                PartRole::Plain,
            ),
            part(
                meshes.add(Cylinder::new(0.25, 0.7)),
                &steel,
                Vec3::new(0.0, 1.75, 0.0),
                PartRole::Plain,
            ),
            part(
                port_z.clone(),
                &output,
                Vec3::new(0.0, 0.6, -1.35),
                PartRole::OutputPort,
            ),
            part(
                light_mesh.clone(),
                &light_active,
                Vec3::new(0.0, 2.1, 0.0),
                PartRole::Light,
            ),
        ],
    );
    parts.insert(
        BuildingKind::Generator,
        vec![
            part(
                meshes.add(Cuboid::new(2.6, 1.5, 2.6)),
                &blue_gray,
                Vec3::new(0.0, 0.75, 0.0),
                PartRole::Plain,
            ),
            part(
                meshes.add(Cylinder::new(0.8, 0.6)),
                &copper,
                Vec3::new(0.0, 1.8, 0.0),
                PartRole::Plain,
            ),
            part(
                meshes.add(Cylinder::new(0.1, 0.3)),
                &steel,
                Vec3::new(0.0, 2.25, 0.0),
                PartRole::Plain,
            ),
            part(
                light_mesh.clone(),
                &light_active,
                Vec3::new(0.95, 1.7, -0.95),
                PartRole::Light,
            ),
        ],
    );
    parts.insert(
        BuildingKind::Furnace,
        vec![
            part(
                meshes.add(Cuboid::new(2.4, 2.0, 2.4)),
                &brick,
                Vec3::new(0.0, 1.0, 0.0),
                PartRole::Plain,
            ),
            part(
                meshes.add(Cuboid::new(1.0, 0.7, 0.12)),
                &mouth_idle,
                Vec3::new(0.0, 0.7, -1.22),
                PartRole::Mouth,
            ),
            part(
                meshes.add(Cylinder::new(0.3, 1.2)),
                &steel,
                Vec3::new(0.7, 2.6, 0.7),
                PartRole::Plain,
            ),
            part(
                port_z.clone(),
                &input,
                Vec3::new(0.0, 0.7, 1.35),
                PartRole::InputPort,
            ),
            part(
                port_x,
                &output,
                Vec3::new(1.35, 0.7, 0.0),
                PartRole::OutputPort,
            ),
            part(
                light_mesh,
                &light_waiting,
                Vec3::new(-0.8, 2.15, -0.8),
                PartRole::Light,
            ),
        ],
    );
    parts.insert(
        BuildingKind::Storage,
        vec![
            part(
                meshes.add(Cuboid::new(1.8, 1.1, 1.8)),
                &crate_wood,
                Vec3::new(0.0, 0.55, 0.0),
                PartRole::Plain,
            ),
            part(
                meshes.add(Cuboid::new(1.9, 0.15, 1.9)),
                &wood,
                Vec3::new(0.0, 1.15, 0.0),
                PartRole::Plain,
            ),
            part(
                port_z.clone(),
                &input,
                Vec3::new(0.0, 0.5, 0.85),
                PartRole::InputPort,
            ),
            part(
                port_z,
                &output,
                Vec3::new(0.0, 0.5, -0.85),
                PartRole::OutputPort,
            ),
        ],
    );
    parts.insert(
        BuildingKind::Pole,
        vec![
            part(
                meshes.add(Cylinder::new(0.12, 4.0)),
                &wood,
                Vec3::new(0.0, 2.0, 0.0),
                PartRole::Plain,
            ),
            part(
                meshes.add(Cuboid::new(0.9, 0.1, 0.1)),
                &wood,
                Vec3::new(0.0, 3.85, 0.0),
                PartRole::Plain,
            ),
            part(
                meshes.add(Cuboid::new(0.12, 0.2, 0.12)),
                &steel,
                Vec3::new(0.0, 4.05, 0.0),
                PartRole::Plain,
            ),
        ],
    );

    let item_materials = [
        (ItemKind::IronOre, Color::srgb(0.50, 0.26, 0.18)),
        (ItemKind::CopperOre, Color::srgb(0.78, 0.46, 0.20)),
        (ItemKind::IronPlate, Color::srgb(0.70, 0.72, 0.76)),
        (ItemKind::CopperPlate, Color::srgb(0.90, 0.55, 0.35)),
    ]
    .into_iter()
    .map(|(kind, color)| (kind, plain(&mut materials, color)))
    .collect();

    commands.insert_resource(BuildingAssets {
        unit_cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        cable: meshes.add(Cuboid::new(0.05, 0.05, 1.0)),
        belt: meshes.add(Cuboid::new(0.6, 0.08, 1.0)),
        item: meshes.add(Cuboid::new(0.3, 0.2, 0.3)),
        parts,
        foundation_material: plain(&mut materials, Color::srgb(0.62, 0.62, 0.60)),
        cable_material: plain(&mut materials, Color::srgb(0.08, 0.08, 0.08)),
        belt_material: plain(&mut materials, Color::srgb(0.18, 0.18, 0.20)),
        item_materials,
        light_active,
        light_waiting,
        light_no_power,
        light_off,
        mouth_idle,
        mouth_hot,
        ghost_valid: ghost(&mut materials, Color::srgba(0.3, 0.95, 0.45, 0.40)),
        ghost_invalid: ghost(&mut materials, Color::srgba(1.0, 0.25, 0.2, 0.40)),
        ghost_input: ghost(&mut materials, Color::srgba(0.25, 0.55, 1.0, 0.9)),
        ghost_output: ghost(&mut materials, Color::srgba(1.0, 0.85, 0.15, 0.9)),
    });
}

fn building_transform(b: &PlacedBuilding) -> Transform {
    Transform::from_xyz(b.x, b.base_y, b.z)
        .with_rotation(Quat::from_rotation_y(f32::from(b.rotation) * FRAC_PI_2))
}

fn foundation_transform(b: &PlacedBuilding) -> Transform {
    let height = (b.base_y - b.foundation_bottom).max(0.01);
    let side = 2.0 * b.dims().half;
    Transform::from_xyz(0.0, -height / 2.0, 0.0).with_scale(Vec3::new(side, height, side))
}

/// Segment droit de `from` à `to` pour un maillage de longueur 1 le long de Z.
fn segment_transform(from: Vec3, to: Vec3) -> Transform {
    let length = from.distance(to).max(0.01);
    Transform::from_translation((from + to) / 2.0)
        .looking_at(to, Vec3::Y)
        .with_scale(Vec3::new(1.0, 1.0, length))
}

/// Un fantôme par type de bâtiment ; seul celui de l'outil choisi est affiché.
fn spawn_ghosts(mut commands: Commands, assets: Res<BuildingAssets>) {
    for kind in BuildingKind::ALL {
        commands
            .spawn((
                Name::new(format!("Fantôme : {}", kind.name())),
                Ghost(kind),
                Transform::default(),
                Visibility::Hidden,
            ))
            .with_children(|ghost| {
                ghost.spawn((
                    GhostPart,
                    Foundation,
                    NotShadowCaster,
                    Mesh3d(assets.unit_cube.clone()),
                    MeshMaterial3d(assets.ghost_valid.clone()),
                    Transform::default(),
                ));
                for part in &assets.parts[&kind] {
                    let material = match part.role {
                        PartRole::InputPort => &assets.ghost_input,
                        PartRole::OutputPort => &assets.ghost_output,
                        _ => &assets.ghost_valid,
                    };
                    let mut entity = ghost.spawn((
                        GhostPart,
                        NotShadowCaster,
                        Mesh3d(part.mesh.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::from_translation(part.offset),
                    ));
                    if matches!(part.role, PartRole::InputPort | PartRole::OutputPort) {
                        entity.insert(GhostPort);
                    }
                }
            });
    }
    commands.spawn((
        Name::new("Fantôme : convoyeur"),
        ConveyorGhost,
        NotShadowCaster,
        Mesh3d(assets.belt.clone()),
        MeshMaterial3d(assets.ghost_valid.clone()),
        Transform::default(),
        Visibility::Hidden,
    ));
}

fn spawn_hud(mut commands: Commands) {
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexEnd,
            padding: UiRect::bottom(Val::Px(24.0)),
            ..default()
        })
        .with_child((
            BuildHud,
            Text::new(""),
            TextFont::from_font_size(17.0),
            TextColor(Color::WHITE),
            TextLayout::justify(Justify::Center),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            Node {
                padding: UiRect::all(Val::Px(10.0)),
                ..default()
            },
            Visibility::Hidden,
        ));
}

fn rotate_key(keys: Res<ButtonInput<KeyCode>>, mut build: ResMut<BuildMode>) {
    if build.active && keys.just_pressed(KeyCode::KeyR) {
        build.rotation = (build.rotation + 1) % 4;
    }
}

fn update_preview(
    build: Res<BuildMode>,
    target: Res<Target>,
    terrain: Res<Terrain>,
    deposits: Res<Deposits>,
    rocks: Res<Rocks>,
    sim: Res<Simulation>,
    buildings: Res<Buildings>,
    player: Single<&Transform, With<Player>>,
    mut current: ResMut<Preview>,
) {
    *current = Preview::default();
    if !build.active {
        return;
    }
    match build.tool {
        BuildTool::Building(kind) => {
            if let Aim::Ground { point, .. } = target.0 {
                let ctx = PlacementContext {
                    field: &terrain.0,
                    deposits: &deposits.0,
                    rocks: &rocks.0,
                    sim: &sim.world,
                    placed: &buildings.0,
                    player: (player.translation, PLAYER_RADIUS),
                };
                current.building = Some(preview(kind, point, build.rotation, &ctx));
            }
        }
        BuildTool::Conveyor => {
            let Some(from) = build.conveyor_from else {
                return;
            };
            // Point visé pour l'aperçu quand on ne vise pas un bâtiment.
            let aim_point = match target.0 {
                Aim::Ground { point, .. } => point,
                _ => player.translation + player.forward().as_vec3() * 6.0,
            };
            current.conveyor = preview_conveyor(
                from,
                target.0.building(),
                aim_point,
                &sim.world,
                &buildings.0,
                &terrain.0,
            );
        }
    }
}

fn update_ghosts(
    preview: Res<Preview>,
    assets: Res<BuildingAssets>,
    mut ghosts: Query<(&Ghost, &mut Transform, &mut Visibility, &Children)>,
    mut parts: Query<
        (
            &mut MeshMaterial3d<StandardMaterial>,
            Has<Foundation>,
            Has<GhostPort>,
            &mut Transform,
        ),
        (With<GhostPart>, Without<Ghost>),
    >,
    belt: Single<
        (
            &mut Transform,
            &mut Visibility,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        (With<ConveyorGhost>, Without<Ghost>, Without<GhostPart>),
    >,
) {
    let material_for = |ok: bool| {
        if ok {
            &assets.ghost_valid
        } else {
            &assets.ghost_invalid
        }
    };
    for (Ghost(kind), mut transform, mut visibility, children) in &mut ghosts {
        let Some(p) = preview
            .building
            .as_ref()
            .filter(|p| p.building.kind == *kind)
        else {
            if *visibility != Visibility::Hidden {
                *visibility = Visibility::Hidden;
            }
            continue;
        };
        *visibility = Visibility::Inherited;
        *transform = building_transform(&p.building);
        let material = material_for(p.result.is_ok());
        for child in children {
            if let Ok((mut part_material, is_foundation, is_port, mut part_transform)) =
                parts.get_mut(*child)
            {
                if !is_port && part_material.0 != *material {
                    part_material.0 = material.clone();
                }
                if is_foundation {
                    *part_transform = foundation_transform(&p.building);
                }
            }
        }
    }

    let (mut transform, mut visibility, mut material) = belt.into_inner();
    match &preview.conveyor {
        Some(c) if c.start.distance(c.end) > 0.1 => {
            *visibility = Visibility::Inherited;
            *transform = segment_transform(c.start, c.end);
            let wanted = material_for(c.result.is_ok());
            if material.0 != *wanted {
                material.0 = wanted.clone();
            }
        }
        _ => {
            if *visibility != Visibility::Hidden {
                *visibility = Visibility::Hidden;
            }
        }
    }
}

fn place(
    mouse: Res<ButtonInput<MouseButton>>,
    mut build: ResMut<BuildMode>,
    preview: Res<Preview>,
    target: Res<Target>,
    deposits: Res<Deposits>,
    assets: Res<BuildingAssets>,
    mut commands: Commands,
    mut sim: ResMut<Simulation>,
    mut buildings: ResMut<Buildings>,
    mut entities: ResMut<BuildingEntities>,
) {
    if !build.active || !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    match build.tool {
        BuildTool::Conveyor => place_conveyor(&mut build, &preview, &target, &mut sim),
        BuildTool::Building(_) => {
            let Some(preview) = &preview.building else {
                return;
            };
            if let Err(error) = preview.result {
                info!("Pose refusée : {}", error.message());
                return;
            }
            let building = preview.building.clone();
            let id = match building.kind {
                BuildingKind::Extractor => {
                    let Some(deposit) = preview.deposit.and_then(|id| deposits.0.get(id)) else {
                        return;
                    };
                    sim.world.add_extractor(deposit)
                }
                kind => match sim.world.add_building(kind, building.x, building.z) {
                    Some(id) => id,
                    None => return,
                },
            };
            let entity = spawn_building(&mut commands, &assets, id, &building);
            entities.0.insert(id, entity);
            info!(
                "{} #{} posé en ({:.0}, {:.0}) · relié à {} bâtiment(s)",
                building.kind.name(),
                id.0,
                building.x,
                building.z,
                preview.connections
            );
            buildings.0.insert(id, building);
        }
    }
}

/// Premier clic : choisir la sortie de départ. Second clic : choisir l'entrée d'arrivée.
fn place_conveyor(build: &mut BuildMode, preview: &Preview, target: &Target, sim: &mut Simulation) {
    match build.conveyor_from {
        None => {
            let Some(id) = target.0.building() else {
                return;
            };
            match sim.world.output_free(id) {
                Ok(()) => build.conveyor_from = Some(id),
                Err(error) => info!("Convoyeur : {}", error.message()),
            }
        }
        Some(from) => {
            let Some(p) = &preview.conveyor else {
                return;
            };
            if let Err(issue) = p.result {
                info!("Convoyeur refusé : {}", issue.message());
                return;
            }
            let Some(to) = p.to else {
                return;
            };
            match sim.world.add_conveyor(from, to, p.start.distance(p.end)) {
                Ok(id) => {
                    info!(
                        "Convoyeur #{} posé de #{} vers #{} ({:.1} m)",
                        id.0,
                        from.0,
                        to.0,
                        p.start.distance(p.end)
                    );
                    build.conveyor_from = None;
                }
                Err(error) => info!("Convoyeur refusé : {}", error.message()),
            }
        }
    }
}

fn spawn_building(
    commands: &mut Commands,
    assets: &BuildingAssets,
    id: BuildingId,
    building: &PlacedBuilding,
) -> Entity {
    commands
        .spawn((
            Name::new(format!("{} #{}", building.kind.name(), id.0)),
            building_transform(building),
            Visibility::default(),
        ))
        .with_children(|root| {
            root.spawn((
                Mesh3d(assets.unit_cube.clone()),
                MeshMaterial3d(assets.foundation_material.clone()),
                foundation_transform(building),
            ));
            for part in &assets.parts[&building.kind] {
                let mut entity = root.spawn((
                    Mesh3d(part.mesh.clone()),
                    MeshMaterial3d(part.material.clone()),
                    Transform::from_translation(part.offset),
                ));
                match part.role {
                    PartRole::Light => {
                        entity.insert(StatusLight(id));
                    }
                    PartRole::Mouth => {
                        entity.insert(FurnaceMouth(id));
                    }
                    PartRole::Plain | PartRole::InputPort | PartRole::OutputPort => {}
                }
            }
        })
        .id()
}

/// Clic droit : annule un convoyeur en cours, sinon démonte ce qui est visé.
fn remove(
    mouse: Res<ButtonInput<MouseButton>>,
    mut build: ResMut<BuildMode>,
    mut target: ResMut<Target>,
    mut commands: Commands,
    mut sim: ResMut<Simulation>,
    mut buildings: ResMut<Buildings>,
    mut entities: ResMut<BuildingEntities>,
) {
    if !build.active || !mouse.just_pressed(MouseButton::Right) {
        return;
    }
    if build.conveyor_from.take().is_some() {
        info!("Convoyeur annulé");
        return;
    }
    match target.0 {
        Aim::Conveyor { id, .. } => {
            if let Some(conveyor) = sim.world.remove_conveyor(id) {
                info!(
                    "Convoyeur #{} démonté ({} objets perdus, mode créatif)",
                    id.0,
                    conveyor.len()
                );
            }
        }
        Aim::Building { id, .. } => {
            let removed = sim.world.remove(id);
            buildings.0.remove(id);
            if let Some(entity) = entities.0.remove(&id) {
                commands.entity(entity).despawn();
            }
            if let Some(building) = removed {
                let lost = match &building.machine {
                    Machine::Extractor(e) => e.output().count(),
                    Machine::Furnace(f) => f.input().count() + f.output().count(),
                    Machine::Storage(inventory) => {
                        ItemKind::ALL.iter().map(|k| inventory.count(*k)).sum()
                    }
                    _ => 0,
                };
                info!(
                    "{} #{} démonté{}",
                    building.machine.kind().name(),
                    id.0,
                    if lost > 0 {
                        format!(" ({lost} objets perdus, mode créatif)")
                    } else {
                        String::new()
                    }
                );
            }
        }
        _ => return,
    }
    target.0 = Aim::Nothing;
}

/// Voyants d'état et gueule des fours, d'après la simulation.
fn update_status_parts(
    sim: Res<Simulation>,
    assets: Res<BuildingAssets>,
    mut lights: Query<(&StatusLight, &mut MeshMaterial3d<StandardMaterial>), Without<FurnaceMouth>>,
    mut mouths: Query<(&FurnaceMouth, &mut MeshMaterial3d<StandardMaterial>), Without<StatusLight>>,
) {
    let state_light = |state: MachineState| match state {
        MachineState::Active => &assets.light_active,
        MachineState::WaitingInput | MachineState::OutputFull => &assets.light_waiting,
        MachineState::NoPower => &assets.light_no_power,
        MachineState::Off => &assets.light_off,
    };
    for (StatusLight(id), mut material) in &mut lights {
        let wanted = match sim.world.building(*id).map(|b| &b.machine) {
            Some(Machine::Extractor(e)) => state_light(e.state()),
            Some(Machine::Furnace(f)) => state_light(f.state()),
            Some(Machine::Generator(g)) if g.enabled => &assets.light_active,
            _ => &assets.light_off,
        };
        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
    }
    for (FurnaceMouth(id), mut material) in &mut mouths {
        let hot = sim
            .world
            .furnace(*id)
            .is_some_and(|f| f.state() == MachineState::Active);
        let wanted = if hot {
            &assets.mouth_hot
        } else {
            &assets.mouth_idle
        };
        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
    }
}

/// Redessine les câbles quand le réseau électrique a été recalculé.
fn update_cables(
    sim: Res<Simulation>,
    buildings: Res<Buildings>,
    assets: Res<BuildingAssets>,
    mut drawn_version: Local<u64>,
    cables: Query<Entity, With<Cable>>,
    mut commands: Commands,
) {
    let version = sim.world.grid_version();
    if version == *drawn_version {
        return;
    }
    *drawn_version = version;
    for cable in &cables {
        commands.entity(cable).despawn();
    }
    for (a, b) in sim.world.grid().edges() {
        let (Some(a), Some(b)) = (buildings.0.get(*a), buildings.0.get(*b)) else {
            continue;
        };
        commands.spawn((
            Name::new("Câble"),
            Cable,
            NotShadowCaster,
            Mesh3d(assets.cable.clone()),
            MeshMaterial3d(assets.cable_material.clone()),
            segment_transform(a.cable_anchor(), b.cable_anchor()),
        ));
    }
}

/// Crée l'affichage des nouveaux convoyeurs et retire celui des convoyeurs disparus
/// (démontés, ou retirés avec un bâtiment).
fn sync_conveyors(
    sim: Res<Simulation>,
    buildings: Res<Buildings>,
    assets: Res<BuildingAssets>,
    visuals: Query<(Entity, &ConveyorVisual)>,
    mut commands: Commands,
) {
    for (entity, visual) in &visuals {
        if sim.world.conveyor(visual.id).is_none() {
            for item in &visual.pool {
                commands.entity(*item).despawn();
            }
            commands.entity(entity).despawn();
        }
    }
    for (id, conveyor) in sim.world.conveyors() {
        if visuals.iter().any(|(_, v)| v.id == id) {
            continue;
        }
        let Some((from, to)) = buildings.0.conveyor_ends(conveyor.from, conveyor.to) else {
            continue;
        };
        let pool = (0..conveyor.capacity())
            .map(|_| {
                commands
                    .spawn((
                        NotShadowCaster,
                        Mesh3d(assets.item.clone()),
                        MeshMaterial3d(assets.item_materials[&ItemKind::IronOre].clone()),
                        Transform::default(),
                        Visibility::Hidden,
                    ))
                    .id()
            })
            .collect();
        let length = from.distance(to);
        commands
            .spawn((
                Name::new(format!("Convoyeur #{}", id.0)),
                ConveyorVisual { id, length, pool },
                Transform::from_translation((from + to) / 2.0).looking_at(to, Vec3::Y),
                Visibility::default(),
            ))
            .with_child((
                Mesh3d(assets.belt.clone()),
                MeshMaterial3d(assets.belt_material.clone()),
                Transform::from_scale(Vec3::new(1.0, 1.0, length)),
            ));
    }
}

/// Place les cubes d'objets le long des tapis, d'après leur avancement dans la simulation.
fn update_conveyor_items(
    sim: Res<Simulation>,
    assets: Res<BuildingAssets>,
    visuals: Query<(&ConveyorVisual, &Transform)>,
    mut items: Query<
        (
            &mut Transform,
            &mut Visibility,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        Without<ConveyorVisual>,
    >,
) {
    for (visual, root) in &visuals {
        let Some(conveyor) = sim.world.conveyor(visual.id) else {
            continue;
        };
        let mut on_belt = conveyor.items();
        for entity in &visual.pool {
            let Ok((mut transform, mut visibility, mut material)) = items.get_mut(*entity) else {
                continue;
            };
            match on_belt.next() {
                Some((kind, progress)) => {
                    // La racine regarde vers l'arrivée (-Z local) : départ à +Z/2, arrivée à -Z/2.
                    let local = Vec3::new(0.0, 0.14, visual.length * (0.5 - progress));
                    transform.translation = root.transform_point(local);
                    transform.rotation = root.rotation;
                    *visibility = Visibility::Inherited;
                    let wanted = &assets.item_materials[&kind];
                    if material.0 != *wanted {
                        material.0 = wanted.clone();
                    }
                }
                None => {
                    if *visibility != Visibility::Hidden {
                        *visibility = Visibility::Hidden;
                    }
                }
            }
        }
    }
}

fn update_hud(
    build: Res<BuildMode>,
    preview: Res<Preview>,
    deposits: Res<Deposits>,
    sim: Res<Simulation>,
    hud: Single<(&mut Text, &mut Visibility), With<BuildHud>>,
) {
    let (mut text, mut visibility) = hud.into_inner();
    if !build.active {
        if *visibility != Visibility::Hidden {
            *visibility = Visibility::Hidden;
        }
        return;
    }
    *visibility = Visibility::Inherited;
    let facing = ["nord (-Z)", "ouest (-X)", "sud (+Z)", "est (+X)"][usize::from(build.rotation)];

    let (title, status) = match build.tool {
        BuildTool::Conveyor => {
            let title = "CONVOYEUR".to_string();
            let status = match (build.conveyor_from, &preview.conveyor) {
                (None, _) => {
                    "Clic sur un bâtiment pour partir de sa sortie (bloc jaune)".to_string()
                }
                (Some(from), p) => {
                    let name = sim
                        .world
                        .building(from)
                        .map_or("?", |b| b.machine.kind().name());
                    let detail = match p {
                        Some(p) if p.result.is_ok() => {
                            let length = p.start.distance(p.end);
                            format!(
                                "OK : {length:.1} m, {:.0} s de trajet",
                                length / CONVEYOR_SPEED_MPS
                            )
                        }
                        Some(p) => format!(
                            "Impossible : {}",
                            p.result.err().map_or("", |issue| issue.message())
                        ),
                        None => String::new(),
                    };
                    format!(
                        "Depuis {name} #{} : vise l'entrée (bloc bleu) d'un four ou d'un coffre\n{detail} · Clic droit : annuler",
                        from.0
                    )
                }
            };
            (title, status)
        }
        BuildTool::Building(kind) => {
            let footprint = 2.0 * dims(kind).half;
            let ports = match (kind.has_input(), kind.has_output()) {
                (true, true) => " · bleu : entrée, jaune : sortie",
                (false, true) => " · jaune : sortie",
                _ => "",
            };
            let title = format!(
                "{} ({footprint:.1} × {footprint:.1} m) · orienté vers le {facing}{ports}",
                kind.name().to_uppercase()
            );
            let status = match &preview.building {
                None => "Vise le sol pour placer le bâtiment".to_string(),
                Some(p) => match p.result {
                    Err(error) => format!("Impossible : {}", error.message()),
                    Ok(()) => {
                        let deposit = p
                            .deposit
                            .and_then(|id| deposits.0.get(id))
                            .filter(|_| kind == BuildingKind::Extractor)
                            .map(|d| {
                                format!(
                                    "gisement de {} {} ({}/min) · ",
                                    d.kind.name(),
                                    d.purity.name(),
                                    extractor_rate_per_min(d.purity)
                                )
                            })
                            .unwrap_or_default();
                        let link = match (kind, p.connections) {
                            (BuildingKind::Storage, _) => String::new(),
                            (BuildingKind::Pole, 0) => "aucun bâtiment à portée".to_string(),
                            (BuildingKind::Pole, n) => format!("reliera {n} bâtiment(s)"),
                            (_, 0) => "aucun poteau à portée : pense à en poser un".to_string(),
                            (_, n) => format!("relié à {n} poteau(x)"),
                        };
                        format!("OK : {deposit}{link}")
                    }
                },
            };
            (title, status)
        }
    };
    text.0 = format!(
        "{title}\n{status}\n\
         Clic gauche : poser · R : tourner · Clic droit : démonter · B : menu · Échap : arrêter"
    );
}
