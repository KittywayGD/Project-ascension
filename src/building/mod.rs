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
use crate::game::GameSession;
use crate::game::{Screen, WorldSetup, reset};
use crate::interaction::{Aim, Target, update_target};
use crate::inventory::ItemKind;
use crate::player::PlayerInventory;
use crate::player::{PLAYER_RADIUS, Player};
use crate::settings::Settings;
use crate::simulation::Simulation;
use crate::simulation::energy::{GENERATOR_POWER_W, POLE_RANGE_M};
use crate::simulation::logistics::{
    CONVEYOR_MAX_LENGTH_M, CONVEYOR_SPEED_MPS, conveyor_max_rate_per_min,
};
use crate::simulation::modules::ModuleKind;
use crate::simulation::production::{
    EXTRACTOR_POWER_W, FURNACE_POWER_W, MachineState, extractor_rate_per_min,
};
use crate::simulation::sim_world::{BuildingId, BuildingKind, Machine, STORAGE_SLOTS, SimWorld};
use crate::simulation::tech::{
    self, Cost, TIERS, Unlock, building_cost, conveyor_cost, describe, is_unlocked, module_cost,
    tier_of,
};
use crate::world::sky::GLOW;
use crate::world::{Deposits, Rocks, Terrain};
use placement::{
    ConveyorPreview, MODULE_HALF, ModulePreview, PlacedBuilding, PlacedBuildings, PlacementContext,
    PlacementPreview, dims, preview, preview_conveyor, preview_module,
};

/// Outil de construction choisi dans le menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildTool {
    Building(BuildingKind),
    Conveyor,
    Module(ModuleKind),
}

impl BuildTool {
    /// Ordre d'affichage dans le menu de construction.
    pub const ALL: [BuildTool; 9] = [
        BuildTool::Building(BuildingKind::Extractor),
        BuildTool::Building(BuildingKind::Generator),
        BuildTool::Building(BuildingKind::Furnace),
        BuildTool::Building(BuildingKind::Storage),
        BuildTool::Building(BuildingKind::Pole),
        BuildTool::Building(BuildingKind::Hub),
        BuildTool::Conveyor,
        BuildTool::Module(ModuleKind::Speed),
        BuildTool::Module(ModuleKind::Efficiency),
    ];

    /// Ce que le palier doit débloquer pour pouvoir utiliser cet outil.
    pub fn unlock(self) -> Unlock {
        match self {
            BuildTool::Building(kind) => Unlock::Building(kind),
            BuildTool::Conveyor => Unlock::Conveyor,
            BuildTool::Module(kind) => Unlock::Module(kind),
        }
    }

    /// Coût affiché dans le menu (convoyeur : par tranche de 2 m).
    pub fn cost_label(self) -> String {
        match self {
            BuildTool::Building(kind) => describe(&building_cost(kind)),
            BuildTool::Conveyor => "1 plaque de fer par 2 m".into(),
            BuildTool::Module(kind) => describe(&module_cost(kind)),
        }
    }

    /// Raison pour laquelle l'outil est verrouillé, `None` s'il est disponible.
    pub fn locked_reason(self, sim: &Simulation, creative: bool) -> Option<String> {
        let unlock = self.unlock();
        if is_unlocked(unlock, sim.world.deliveries(), creative) {
            return None;
        }
        let tier = &TIERS[tier_of(unlock)];
        let needs: Vec<String> = tier
            .requires
            .iter()
            .map(|&(kind, amount)| kind.quantity(amount as u32))
            .collect();
        Some(format!(
            "verrouillé : palier « {} » (livrer {} au hub)",
            tier.name,
            needs.join(" et ")
        ))
    }

    pub fn name(self) -> &'static str {
        match self {
            BuildTool::Building(kind) => kind.name(),
            BuildTool::Conveyor => "Convoyeur",
            BuildTool::Module(kind) => kind.name(),
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
            BuildTool::Building(BuildingKind::Hub) => "accepte tout · paliers".to_string(),
            BuildTool::Conveyor => format!(
                "{}/min max · {CONVEYOR_MAX_LENGTH_M:.0} m max",
                conveyor_max_rate_per_min()
            ),
            BuildTool::Module(kind) => format!(
                "vitesse {:+} % · consommation {:+} %",
                kind.speed_bonus(),
                kind.power_bonus()
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
                "Fond le minerai en plaques. Entrée à l'arrière, sortie à l'avant."
            }
            BuildTool::Building(BuildingKind::Storage) => {
                "Stocke tout ce qu'on lui apporte. Entrée à l'arrière, sortie à l'avant."
            }
            BuildTool::Building(BuildingKind::Pole) => {
                "Relie au réseau tout bâtiment à portée, et les autres poteaux."
            }
            BuildTool::Building(BuildingKind::Hub) => {
                "Reçoit les livraisons (entrée à l'arrière), qui débloquent les paliers."
            }
            BuildTool::Conveyor => {
                "Relie une sortie (bloc jaune) à une entrée (bloc bleu). Deux clics."
            }
            BuildTool::Module(ModuleKind::Speed) => {
                "Se fixe sur le toit d'un extracteur ou d'un four : plus rapide, plus gourmand."
            }
            BuildTool::Module(ModuleKind::Efficiency) => {
                "Se fixe sur le toit d'un extracteur ou d'un four : consomme moins."
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

/// La scène de démonstration est-elle demandée au lancement (`--demo`) ?
#[derive(Resource, Debug, Default)]
pub struct DemoRequested(pub bool);

/// Bâtiments posés (données de jeu) et leurs entités d'affichage.
#[derive(Resource, Debug, Default)]
pub struct Buildings(pub PlacedBuildings);

#[derive(Resource, Debug, Default)]
struct BuildingEntities(HashMap<BuildingId, Entity>);

/// Faut-il charger (`Some(true)`) ou décharger (`Some(false)`) l'affichage d'un bâtiment
/// à cette distance horizontale du joueur ? `None` : ne rien changer.
/// `load_m` / `unload_m` : distances réglées dans les options (150 / 170 m par défaut).
pub fn visibility_change(distance: f32, loaded: bool, load_m: f32, unload_m: f32) -> Option<bool> {
    match (loaded, distance) {
        (false, d) if d <= load_m => Some(true),
        (true, d) if d > unload_m => Some(false),
        _ => None,
    }
}

/// Bilan de l'affichage : seuls les bâtiments proches ont des entités, la simulation
/// continue pour tous (M6).
#[derive(Resource, Debug, Default)]
pub struct StreamStats {
    pub loaded: usize,
    pub total: usize,
    /// Incrémenté à chaque chargement / déchargement (pour redessiner les câbles).
    version: u64,
}

/// Aperçu de pose courant (pour le fantôme et le bandeau d'aide).
#[derive(Resource, Debug, Default)]
pub struct Preview {
    pub building: Option<PlacementPreview>,
    pub conveyor: Option<ConveyorPreview>,
    pub module: Option<ModulePreview>,
    /// Prix de ce qui est visé (vide en mode créatif ou s'il n'y a rien à poser).
    pub cost: Cost,
    /// Verrouillé, ou pas assez de plaques : la pose est refusée pour cette raison.
    pub blocked: Option<String>,
}

/// Résultat de la dernière pose ou du dernier démontage, affiché quelques secondes.
#[derive(Resource, Debug, Default)]
pub struct BuildFeedback {
    pub text: String,
    pub until: f32,
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
    rotation: Quat,
    role: PartRole,
}

#[derive(Resource)]
struct BuildingAssets {
    unit_cube: Handle<Mesh>,
    cable: Handle<Mesh>,
    belt: Handle<Mesh>,
    belt_rail: Handle<Mesh>,
    /// Flèche peinte sur le tapis, pointe vers -Z local (sens de déplacement).
    chevron: Handle<Mesh>,
    item: Handle<Mesh>,
    parts: HashMap<BuildingKind, Vec<Part>>,
    foundation_material: Handle<StandardMaterial>,
    cable_material: Handle<StandardMaterial>,
    belt_material: Handle<StandardMaterial>,
    rail_material: Handle<StandardMaterial>,
    chevron_material: Handle<StandardMaterial>,
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
    module_body: Handle<Mesh>,
    module_cap: Handle<Mesh>,
    module_materials: HashMap<ModuleKind, Handle<StandardMaterial>>,
    module_glow: HashMap<ModuleKind, Handle<StandardMaterial>>,
    module_idle: Handle<StandardMaterial>,
}

#[derive(Component)]
struct Ghost(BuildingKind);

#[derive(Component)]
struct ConveyorGhost;

#[derive(Component)]
struct ModuleGhost;

/// Affichage d'un module posé.
#[derive(Component)]
struct ModuleVisual(BuildingId);

/// Pastille d'un module, allumée quand sa machine travaille.
#[derive(Component)]
struct ModuleIndicator(BuildingId);

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
            .init_resource::<StreamStats>()
            .init_resource::<BuildFeedback>()
            .init_resource::<DemoRequested>()
            .add_systems(Startup, setup_assets)
            .add_systems(
                OnEnter(Screen::InGame),
                build_demo_scene.in_set(WorldSetup::Restore),
            )
            .add_systems(
                OnEnter(Screen::InGame),
                (spawn_ghosts, spawn_hud).in_set(WorldSetup::Spawn),
            )
            .add_systems(
                OnExit(Screen::InGame),
                (
                    reset::<BuildMode>,
                    reset::<Buildings>,
                    reset::<BuildingEntities>,
                    reset::<Preview>,
                    reset::<StreamStats>,
                    reset::<BuildFeedback>,
                ),
            )
            .add_systems(
                Update,
                (
                    rotate_key,
                    update_preview,
                    check_cost,
                    update_ghosts,
                    place,
                    remove,
                )
                    .chain()
                    .after(update_target)
                    .run_if(in_state(PlayState::Running)),
            )
            .add_systems(
                Update,
                (
                    stream_buildings,
                    update_status_parts,
                    update_cables,
                    sync_conveyors,
                    sync_modules,
                    update_module_indicators,
                    update_conveyor_items,
                    update_hud,
                )
                    .chain()
                    .after(remove)
                    .run_if(in_state(Screen::InGame)),
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
    // Émissions × `GLOW` : l'exposition « plein soleil » les assombrit sinon.
    let glow = |materials: &mut Assets<StandardMaterial>, color: Color, emissive: LinearRgba| {
        materials.add(StandardMaterial {
            base_color: color,
            emissive: emissive * GLOW,
            ..default()
        })
    };
    let metal = |materials: &mut Assets<StandardMaterial>, color: Color| {
        materials.add(StandardMaterial {
            base_color: color,
            metallic: 0.8,
            perceptual_roughness: 0.35,
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

    // Palette par famille (voir docs/visual-style.md) :
    // extraction = orange, énergie = violet, transformation = brique,
    // logistique = bois, livraison (hub) = blanc et or. Ports : bleu = entrée, jaune = sortie.
    let orange = plain(&mut materials, Color::srgb(0.88, 0.47, 0.13));
    let steel = plain(&mut materials, Color::srgb(0.36, 0.37, 0.40));
    let silver = metal(&mut materials, Color::srgb(0.78, 0.79, 0.82));
    let output = plain(&mut materials, Color::srgb(0.98, 0.80, 0.15));
    let input = plain(&mut materials, Color::srgb(0.18, 0.48, 0.98));
    let energy = plain(&mut materials, Color::srgb(0.38, 0.30, 0.66));
    let energy_dark = plain(&mut materials, Color::srgb(0.22, 0.18, 0.40));
    let brick = plain(&mut materials, Color::srgb(0.60, 0.29, 0.20));
    let wood = plain(&mut materials, Color::srgb(0.45, 0.32, 0.20));
    let crate_wood = plain(&mut materials, Color::srgb(0.66, 0.50, 0.29));
    let hub_white = plain(&mut materials, Color::srgb(0.90, 0.89, 0.85));
    let hub_gold = metal(&mut materials, Color::srgb(0.92, 0.72, 0.28));
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
        rotation: Quat::IDENTITY,
        role,
    };
    // Cylindre couché le long de l'axe X local (tambour de turbine).
    let lying = |mesh: Handle<Mesh>, material: &Handle<StandardMaterial>, offset: Vec3| Part {
        mesh,
        material: material.clone(),
        offset,
        rotation: Quat::from_rotation_z(FRAC_PI_2),
        role: PartRole::Plain,
    };
    let light_mesh = meshes.add(Sphere::new(0.24));
    // Ports : bloc jaune = sortie, bloc bleu = entrée (même position que dans `placement.rs`).
    let port_z = meshes.add(Cuboid::new(0.8, 0.8, 0.3));
    let mut parts = HashMap::new();
    // Extracteur : bloc orange et colonne de forage à l'avant (le toit arrière porte
    // les modules). Silhouette : une tour fine qui dépasse.
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
                meshes.add(Cylinder::new(0.2, 0.9)),
                &steel,
                Vec3::new(0.0, 1.85, -0.45),
                PartRole::Plain,
            ),
            part(
                meshes.add(Cylinder::new(0.42, 0.25)),
                &orange,
                Vec3::new(0.0, 1.55, -0.45),
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
                Vec3::new(0.0, 2.35, -0.45),
                PartRole::Light,
            ),
        ],
    );
    // Générateur : caisson violet (énergie) et tambour argenté couché.
    parts.insert(
        BuildingKind::Generator,
        vec![
            part(
                meshes.add(Cuboid::new(2.6, 1.5, 2.6)),
                &energy,
                Vec3::new(0.0, 0.75, 0.0),
                PartRole::Plain,
            ),
            part(
                meshes.add(Cuboid::new(2.0, 0.12, 1.4)),
                &energy_dark,
                Vec3::new(0.0, 1.56, 0.0),
                PartRole::Plain,
            ),
            lying(
                meshes.add(Cylinder::new(0.42, 1.9)),
                &silver,
                Vec3::new(0.0, 1.98, 0.0),
            ),
            lying(
                meshes.add(Cylinder::new(0.46, 0.12)),
                &energy_dark,
                Vec3::new(-0.6, 1.98, 0.0),
            ),
            lying(
                meshes.add(Cylinder::new(0.46, 0.12)),
                &energy_dark,
                Vec3::new(0.6, 1.98, 0.0),
            ),
            part(
                meshes.add(Cylinder::new(0.1, 0.5)),
                &steel,
                Vec3::new(0.95, 1.75, 0.95),
                PartRole::Plain,
            ),
            part(
                light_mesh.clone(),
                &light_active,
                Vec3::new(-0.95, 1.75, -0.95),
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
                // Gueule sur le côté gauche : l'avant porte la sortie, l'arrière l'entrée.
                meshes.add(Cuboid::new(0.12, 0.7, 1.0)),
                &mouth_idle,
                Vec3::new(-1.22, 0.7, 0.0),
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
                port_z.clone(),
                &output,
                Vec3::new(0.0, 0.7, -1.35),
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
                port_z.clone(),
                &output,
                Vec3::new(0.0, 0.5, -0.85),
                PartRole::OutputPort,
            ),
        ],
    );
    parts.insert(
        BuildingKind::Hub,
        vec![
            part(
                meshes.add(Cuboid::new(2.8, 0.6, 2.8)),
                &hub_white,
                Vec3::new(0.0, 0.3, 0.0),
                PartRole::Plain,
            ),
            part(
                meshes.add(Cylinder::new(0.45, 2.4)),
                &hub_white,
                Vec3::new(0.0, 1.8, 0.0),
                PartRole::Plain,
            ),
            part(
                meshes.add(Torus::new(0.55, 0.75)),
                &hub_gold,
                Vec3::new(0.0, 2.6, 0.0),
                PartRole::Plain,
            ),
            part(
                meshes.add(Sphere::new(0.3)),
                &light_waiting,
                Vec3::new(0.0, 3.15, 0.0),
                PartRole::Light,
            ),
            part(
                port_z.clone(),
                &input,
                Vec3::new(0.0, 0.6, 1.35),
                PartRole::InputPort,
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
                meshes.add(Cuboid::new(0.14, 0.22, 0.14)),
                &energy,
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
        belt_rail: meshes.add(Cuboid::new(0.06, 0.14, 1.0)),
        chevron: meshes.add(Triangle3d::new(
            Vec3::new(0.0, 0.0, -0.17),
            Vec3::new(-0.2, 0.0, 0.11),
            Vec3::new(0.2, 0.0, 0.11),
        )),
        item: meshes.add(Cuboid::new(0.3, 0.2, 0.3)),
        parts,
        foundation_material: plain(&mut materials, Color::srgb(0.52, 0.52, 0.50)),
        // Câbles mats : sinon le reflet du ciel les rend beiges.
        cable_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.05, 0.05, 0.05),
            perceptual_roughness: 0.95,
            reflectance: 0.1,
            ..default()
        }),
        belt_material: plain(&mut materials, Color::srgb(0.16, 0.16, 0.18)),
        rail_material: plain(&mut materials, Color::srgb(0.40, 0.41, 0.44)),
        chevron_material: plain(&mut materials, Color::srgb(0.85, 0.68, 0.18)),
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
        module_body: meshes.add(Cuboid::new(
            2.0 * MODULE_HALF.x,
            2.0 * MODULE_HALF.y - 0.1,
            2.0 * MODULE_HALF.z,
        )),
        module_cap: meshes.add(Cuboid::new(0.35, 0.1, 0.35)),
        module_materials: [
            (ModuleKind::Speed, Color::srgb(0.12, 0.50, 0.62)),
            (ModuleKind::Efficiency, Color::srgb(0.22, 0.55, 0.28)),
        ]
        .into_iter()
        .map(|(kind, color)| (kind, plain(&mut materials, color)))
        .collect(),
        module_glow: [
            (ModuleKind::Speed, LinearRgba::rgb(0.3, 2.5, 4.0)),
            (ModuleKind::Efficiency, LinearRgba::rgb(0.6, 4.0, 0.8)),
        ]
        .into_iter()
        .map(|(kind, emissive)| (kind, glow(&mut materials, Color::WHITE, emissive)))
        .collect(),
        module_idle: plain(&mut materials, Color::srgb(0.15, 0.15, 0.17)),
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
                        Transform::from_translation(part.offset).with_rotation(part.rotation),
                    ));
                    if matches!(part.role, PartRole::InputPort | PartRole::OutputPort) {
                        entity.insert(GhostPort);
                    }
                }
            });
    }
    commands.spawn((
        Name::new("Fantôme : module"),
        ModuleGhost,
        NotShadowCaster,
        Mesh3d(assets.module_body.clone()),
        MeshMaterial3d(assets.ghost_valid.clone()),
        Transform::default(),
        Visibility::Hidden,
    ));
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
        BuildTool::Module(kind) => {
            // Machine visée : directement, ou à travers un module déjà posé sur son toit.
            let (host, distance) = match target.0 {
                Aim::Building { id, distance } => (id, distance),
                Aim::Module { id, distance } => match sim.world.module(id) {
                    Some(m) => (m.host, distance),
                    None => return,
                },
                _ => return,
            };
            let hit = player.translation + player.forward().as_vec3() * distance;
            current.module = preview_module(kind, host, hit, &sim.world, &buildings.0);
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

/// Verrou et prix de ce qui est visé : la pose est refusée s'il manque quelque chose.
fn check_cost(
    build: Res<BuildMode>,
    session: Res<GameSession>,
    sim: Res<Simulation>,
    inventory: Res<PlayerInventory>,
    mut preview: ResMut<Preview>,
) {
    if !build.active {
        return;
    }
    if let Some(reason) = build.tool.locked_reason(&sim, session.creative) {
        preview.blocked = Some(reason);
        return;
    }
    if session.creative {
        return;
    }
    let cost = match build.tool {
        BuildTool::Building(kind) => building_cost(kind),
        BuildTool::Module(kind) => module_cost(kind),
        // Le prix d'un convoyeur dépend de sa longueur : connu une fois l'arrivée visée.
        BuildTool::Conveyor => match &preview.conveyor {
            Some(c) if c.to.is_some() => conveyor_cost(c.start.distance(c.end)),
            _ => return,
        },
    };
    let missing = tech::missing(&inventory.0, &cost);
    if !missing.is_empty() {
        preview.blocked = Some(format!(
            "il manque {} (coût : {})",
            describe(&missing),
            describe(&cost)
        ));
    }
    preview.cost = cost;
}

/// Objets contenus dans une machine (rendus au joueur quand on la démonte).
fn machine_contents(machine: &Machine) -> Cost {
    let mut items = Vec::new();
    match machine {
        Machine::Extractor(e) => items.push((e.output().kind(), e.output().count())),
        Machine::Furnace(f) => {
            for stack in [f.input().stack(), f.output().stack()]
                .into_iter()
                .flatten()
            {
                items.push((stack.kind, stack.count));
            }
        }
        Machine::Storage(inventory) => {
            for stack in inventory.slots().iter().flatten() {
                items.push((stack.kind, stack.count));
            }
        }
        Machine::Generator(_) | Machine::Pole | Machine::Hub => {}
    }
    items.retain(|&(_, count)| count > 0);
    items
}

/// Objets d'un convoyeur et, hors mode créatif, son prix : ce que rend son démontage.
fn conveyor_salvage(
    sim: &SimWorld,
    buildings: &PlacedBuildings,
    id: BuildingId,
    creative: bool,
) -> Cost {
    let Some(conveyor) = sim.conveyor(id) else {
        return Vec::new();
    };
    let mut items: Cost = conveyor.items().map(|(kind, _)| (kind, 1)).collect();
    if !creative && let Some((from, to)) = buildings.conveyor_ends(conveyor.from, conveyor.to) {
        items.extend(conveyor_cost(from.distance(to)));
    }
    items
}

/// Rend `items` au joueur et prépare le message : « +10 plaques de fer ».
fn give_back(inventory: &mut PlayerInventory, items: &[(ItemKind, u32)], what: &str) -> String {
    // Regroupe par type pour un message lisible.
    let mut merged: Cost = Vec::new();
    for &(kind, count) in items {
        match merged.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, n)) => *n += count,
            None => merged.push((kind, count)),
        }
    }
    let lost = tech::refund(&mut inventory.0, &merged);
    let mut message = if merged.is_empty() {
        format!("{what} démonté")
    } else {
        format!("{what} démonté : +{}", describe(&merged))
    };
    if !lost.is_empty() {
        message.push_str(&format!(" · inventaire plein, perdu : {}", describe(&lost)));
    }
    message
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
    module_ghost: Single<
        (
            &mut Transform,
            &mut Visibility,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        (
            With<ModuleGhost>,
            Without<ConveyorGhost>,
            Without<Ghost>,
            Without<GhostPart>,
        ),
    >,
) {
    let affordable = preview.blocked.is_none();
    let material_for = |ok: bool| {
        if ok && affordable {
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

    let (mut transform, mut visibility, mut material) = module_ghost.into_inner();
    match &preview.module {
        Some(m) => {
            *visibility = Visibility::Inherited;
            *transform = Transform::from_translation(m.position + Vec3::Y * MODULE_HALF.y);
            let wanted = material_for(m.result.is_ok());
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
    mut inventory: ResMut<PlayerInventory>,
    mut feedback: ResMut<BuildFeedback>,
    time: Res<Time<Real>>,
) {
    if !build.active || !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    if let Some(reason) = &preview.blocked {
        feedback.text = format!("Impossible : {reason}");
        feedback.until = time.elapsed_secs() + 3.0;
        return;
    }
    let cost = preview.cost.clone();
    match build.tool {
        BuildTool::Conveyor => {
            place_conveyor(&mut build, &preview, &target, &mut sim, &mut inventory)
        }
        BuildTool::Module(kind) => {
            let Some(p) = &preview.module else {
                return;
            };
            match p
                .result
                .and_then(|()| sim.world.attach_module(p.host, p.socket, kind))
            {
                Ok(id) => {
                    tech::pay(&mut inventory.0, &cost);
                    info!(
                        "{} #{} fixé sur #{} (emplacement {})",
                        kind.name(),
                        id.0,
                        p.host.0,
                        p.socket + 1
                    );
                }
                Err(error) => info!("Module refusé : {}", error.message()),
            }
        }
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
            tech::pay(&mut inventory.0, &cost);
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
fn place_conveyor(
    build: &mut BuildMode,
    preview: &Preview,
    target: &Target,
    sim: &mut Simulation,
    inventory: &mut PlayerInventory,
) {
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
                    tech::pay(&mut inventory.0, &preview.cost);
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
                    Transform::from_translation(part.offset).with_rotation(part.rotation),
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
/// Le contenu revient dans l'inventaire et, hors mode créatif, le prix est remboursé
/// (y compris les convoyeurs et modules qui partent avec un bâtiment).
fn remove(
    mouse: Res<ButtonInput<MouseButton>>,
    mut build: ResMut<BuildMode>,
    mut target: ResMut<Target>,
    mut commands: Commands,
    mut sim: ResMut<Simulation>,
    mut buildings: ResMut<Buildings>,
    mut entities: ResMut<BuildingEntities>,
    mut inventory: ResMut<PlayerInventory>,
    mut feedback: ResMut<BuildFeedback>,
    session: Res<GameSession>,
    time: Res<Time<Real>>,
) {
    if !build.active || !mouse.just_pressed(MouseButton::Right) {
        return;
    }
    if build.conveyor_from.take().is_some() {
        info!("Convoyeur annulé");
        return;
    }
    let creative = session.creative;
    let message = match target.0 {
        Aim::Module { id, .. } => {
            let Some(module) = sim.world.detach_module(id) else {
                return;
            };
            let refund = if creative {
                Vec::new()
            } else {
                module_cost(module.kind)
            };
            give_back(&mut inventory, &refund, module.kind.name())
        }
        Aim::Conveyor { id, .. } => {
            let salvage = conveyor_salvage(&sim.world, &buildings.0, id, creative);
            if sim.world.remove_conveyor(id).is_none() {
                return;
            }
            give_back(&mut inventory, &salvage, "Convoyeur")
        }
        Aim::Building { id, .. } => {
            let Some(building) = sim.world.building(id) else {
                return;
            };
            let kind = building.machine.kind();
            let mut salvage = machine_contents(&building.machine);
            if !creative {
                salvage.extend(building_cost(kind));
                for (_, module) in sim.world.modules_of(id) {
                    salvage.extend(module_cost(module.kind));
                }
            }
            let attached: Vec<BuildingId> = sim
                .world
                .conveyors()
                .filter(|(_, c)| c.from == id || c.to == id)
                .map(|(cid, _)| cid)
                .collect();
            for conveyor in attached {
                salvage.extend(conveyor_salvage(
                    &sim.world,
                    &buildings.0,
                    conveyor,
                    creative,
                ));
            }
            sim.world.remove(id);
            buildings.0.remove(id);
            if let Some(entity) = entities.0.remove(&id) {
                commands.entity(entity).despawn();
            }
            give_back(&mut inventory, &salvage, kind.name())
        }
        _ => return,
    };
    info!("{message}");
    feedback.text = message;
    feedback.until = time.elapsed_secs() + 3.0;
    target.0 = Aim::Nothing;
}

/// Voyants d'état et gueule des fours, d'après la simulation.
fn update_status_parts(
    sim: Res<Simulation>,
    assets: Res<BuildingAssets>,
    time: Res<Time<Real>>,
    mut lights: Query<(&StatusLight, &mut MeshMaterial3d<StandardMaterial>), Without<FurnaceMouth>>,
    mut mouths: Query<(&FurnaceMouth, &mut MeshMaterial3d<StandardMaterial>), Without<StatusLight>>,
) {
    let blink_on = (time.elapsed_secs() * 1.5).fract() < 0.6;
    let state_light = |state: MachineState| match state {
        MachineState::Active => &assets.light_active,
        MachineState::WaitingInput | MachineState::OutputFull => &assets.light_waiting,
        // Clignote : une panne de courant doit se repérer de loin.
        MachineState::NoPower if blink_on => &assets.light_no_power,
        MachineState::NoPower => &assets.light_off,
        MachineState::Off => &assets.light_off,
    };
    for (StatusLight(id), mut material) in &mut lights {
        let wanted = match sim.world.building(*id).map(|b| &b.machine) {
            Some(Machine::Extractor(e)) => state_light(e.state()),
            Some(Machine::Furnace(f)) => state_light(f.state()),
            Some(Machine::Generator(g)) if g.enabled => &assets.light_active,
            // Hub : vert quand un convoyeur lui apporte quelque chose.
            Some(Machine::Hub) => {
                let receiving = sim
                    .world
                    .conveyors()
                    .any(|(_, c)| c.to == *id && c.len() > 0);
                if receiving {
                    &assets.light_active
                } else {
                    &assets.light_waiting
                }
            }
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
    entities: Res<BuildingEntities>,
    stream: Res<StreamStats>,
    assets: Res<BuildingAssets>,
    mut drawn_version: Local<(u64, u64)>,
    cables: Query<Entity, With<Cable>>,
    mut commands: Commands,
) {
    let version = (sim.world.grid_version(), stream.version);
    if version == *drawn_version {
        return;
    }
    *drawn_version = version;
    for cable in &cables {
        commands.entity(cable).despawn();
    }
    for (a, b) in sim.world.grid().edges() {
        // Seulement entre deux bâtiments affichés.
        if !entities.0.contains_key(a) || !entities.0.contains_key(b) {
            continue;
        }
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
    entities: Res<BuildingEntities>,
    assets: Res<BuildingAssets>,
    visuals: Query<(Entity, &ConveyorVisual)>,
    mut commands: Commands,
) {
    // Un tapis est affiché si ses deux bâtiments le sont.
    let shown = |from: &BuildingId, to: &BuildingId| {
        entities.0.contains_key(from) && entities.0.contains_key(to)
    };
    for (entity, visual) in &visuals {
        let keep = sim
            .world
            .conveyor(visual.id)
            .is_some_and(|c| shown(&c.from, &c.to));
        if !keep {
            for item in &visual.pool {
                commands.entity(*item).despawn();
            }
            commands.entity(entity).despawn();
        }
    }
    for (id, conveyor) in sim.world.conveyors() {
        if !shown(&conveyor.from, &conveyor.to) || visuals.iter().any(|(_, v)| v.id == id) {
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
            .with_children(|belt| {
                belt.spawn((
                    Mesh3d(assets.belt.clone()),
                    MeshMaterial3d(assets.belt_material.clone()),
                    Transform::from_scale(Vec3::new(1.0, 1.0, length)),
                ));
                for side in [-0.33, 0.33] {
                    belt.spawn((
                        Mesh3d(assets.belt_rail.clone()),
                        MeshMaterial3d(assets.rail_material.clone()),
                        Transform::from_xyz(side, 0.03, 0.0)
                            .with_scale(Vec3::new(1.0, 1.0, length)),
                    ));
                }
                // Flèches peintes : le sens du tapis se lit même quand il est vide.
                for z in chevron_offsets(length) {
                    belt.spawn((
                        NotShadowCaster,
                        Mesh3d(assets.chevron.clone()),
                        MeshMaterial3d(assets.chevron_material.clone()),
                        Transform::from_xyz(0.0, 0.045, z),
                    ));
                }
            });
    }
}

/// Espacement des flèches peintes sur les tapis, en mètres.
const CHEVRON_SPACING_M: f32 = 1.5;

/// Positions (z local, la racine regarde vers -Z) des flèches d'un tapis de `length` m :
/// régulières, centrées, à distance des deux extrémités.
fn chevron_offsets(length: f32) -> Vec<f32> {
    let usable = length - 1.0;
    if usable <= 0.0 {
        return Vec::new();
    }
    let count = (usable / CHEVRON_SPACING_M).floor() as usize + 1;
    let span = (count - 1) as f32 * CHEVRON_SPACING_M;
    (0..count)
        .map(|i| span / 2.0 - i as f32 * CHEVRON_SPACING_M)
        .collect()
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
    session: Res<GameSession>,
    inventory: Res<PlayerInventory>,
    feedback: Res<BuildFeedback>,
    time: Res<Time<Real>>,
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
        BuildTool::Module(kind) => {
            let title = format!(
                "{} · vitesse {:+} %, consommation {:+} %",
                kind.name().to_uppercase(),
                kind.speed_bonus(),
                kind.power_bonus()
            );
            let status = match &preview.module {
                None => "Vise un extracteur ou un four (2 emplacements sur le toit)".to_string(),
                Some(p) => match p.result {
                    Ok(()) => format!("OK : emplacement {} sur #{}", p.socket + 1, p.host.0),
                    Err(error) => format!("Impossible : {}", error.message()),
                },
            };
            (title, status)
        }
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
                (true, false) => " · bleu : entrée",
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
    // Verrou ou plaques manquantes : prime sur le reste.
    let status = match &preview.blocked {
        Some(reason) => format!("Impossible : {reason}"),
        None => status,
    };
    let price = if session.creative {
        "gratuit (créatif)".to_string()
    } else {
        let iron = inventory.0.count(ItemKind::IronPlate);
        let copper = inventory.0.count(ItemKind::CopperPlate);
        format!(
            "coût : {} · tu as {iron} fer, {copper} cuivre",
            build.tool.cost_label()
        )
    };
    let recent = if time.elapsed_secs() < feedback.until {
        format!("\n{}", feedback.text)
    } else {
        String::new()
    };
    text.0 = format!(
        "{title}\n{price}\n{status}{recent}\n\
         Clic gauche : poser · R : tourner · Clic droit : démonter (remboursé) · B : menu · Échap : arrêter"
    );
}

/// Crée l'affichage des nouveaux modules et retire celui des modules disparus.
fn sync_modules(
    sim: Res<Simulation>,
    buildings: Res<Buildings>,
    entities: Res<BuildingEntities>,
    assets: Res<BuildingAssets>,
    visuals: Query<(Entity, &ModuleVisual)>,
    mut commands: Commands,
) {
    // Un module est affiché si sa machine l'est.
    for (entity, ModuleVisual(id)) in &visuals {
        let keep = sim
            .world
            .module(*id)
            .is_some_and(|m| entities.0.contains_key(&m.host));
        if !keep {
            commands.entity(entity).despawn();
        }
    }
    for (id, module) in sim.world.modules() {
        if !entities.0.contains_key(&module.host)
            || visuals.iter().any(|(_, ModuleVisual(v))| *v == id)
        {
            continue;
        }
        let Some(host) = buildings.0.get(module.host) else {
            continue;
        };
        let Some(base) = host.socket_world(module.socket) else {
            continue;
        };
        let body_height = 2.0 * MODULE_HALF.y - 0.1;
        commands
            .spawn((
                Name::new(format!("{} #{}", module.kind.name(), id.0)),
                ModuleVisual(id),
                Transform::from_translation(base)
                    .with_rotation(Quat::from_rotation_y(f32::from(host.rotation) * FRAC_PI_2)),
                Visibility::default(),
            ))
            .with_children(|root| {
                root.spawn((
                    Mesh3d(assets.module_body.clone()),
                    MeshMaterial3d(assets.module_materials[&module.kind].clone()),
                    Transform::from_xyz(0.0, body_height / 2.0, 0.0),
                ));
                root.spawn((
                    ModuleIndicator(id),
                    Mesh3d(assets.module_cap.clone()),
                    MeshMaterial3d(assets.module_idle.clone()),
                    Transform::from_xyz(0.0, body_height + 0.05, 0.0),
                ));
            });
    }
}

/// Pastille allumée quand la machine du module travaille, éteinte sinon.
fn update_module_indicators(
    sim: Res<Simulation>,
    assets: Res<BuildingAssets>,
    mut indicators: Query<(&ModuleIndicator, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    for (ModuleIndicator(id), mut material) in &mut indicators {
        let Some(module) = sim.world.module(*id) else {
            continue;
        };
        let active = match sim.world.building(module.host).map(|b| &b.machine) {
            Some(Machine::Extractor(e)) => e.state() == MachineState::Active,
            Some(Machine::Furnace(f)) => f.state() == MachineState::Active,
            _ => false,
        };
        let wanted = if active {
            &assets.module_glow[&module.kind]
        } else {
            &assets.module_idle
        };
        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
    }
}

/// M6 : charge l'affichage des bâtiments proches du joueur et décharge celui des
/// bâtiments lointains. **La simulation n'est jamais touchée** : une usine déchargée
/// continue de produire, et son affichage est recréé à l'identique au retour du joueur.
fn stream_buildings(
    player: Single<&Transform, With<Player>>,
    settings: Res<Settings>,
    buildings: Res<Buildings>,
    assets: Res<BuildingAssets>,
    mut entities: ResMut<BuildingEntities>,
    mut stats: ResMut<StreamStats>,
    mut commands: Commands,
) {
    let p = player.translation;
    let mut changed = false;
    for (id, building) in buildings.0.iter() {
        let distance = (building.x - p.x).hypot(building.z - p.z);
        match visibility_change(
            distance,
            entities.0.contains_key(&id),
            settings.view_distance_m as f32,
            settings.view_unload_m(),
        ) {
            Some(true) => {
                let entity = spawn_building(&mut commands, &assets, id, building);
                entities.0.insert(id, entity);
                changed = true;
            }
            Some(false) => {
                if let Some(entity) = entities.0.remove(&id) {
                    commands.entity(entity).despawn();
                }
                changed = true;
            }
            None => {}
        }
    }
    let (loaded, total) = (entities.0.len(), buildings.0.iter().count());
    if changed || stats.loaded != loaded || stats.total != total {
        stats.loaded = loaded;
        stats.total = total;
        stats.version += 1;
    }
}

/// Construit la scène de démonstration au lancement si `--demo` est passé.
/// L'affichage suit tout seul : `stream_buildings` crée les entités des bâtiments proches.
fn build_demo_scene(
    mut demo: ResMut<DemoRequested>,
    terrain: Res<Terrain>,
    deposits: Res<Deposits>,
    rocks: Res<Rocks>,
    mut sim: ResMut<Simulation>,
    mut buildings: ResMut<Buildings>,
) {
    // Une seule fois : un monde créé ensuite depuis l'écran titre reste vide.
    if !std::mem::take(&mut demo.0) {
        return;
    }
    match crate::scene::build_demo(
        &terrain.0,
        &deposits.0,
        &rocks.0,
        &mut sim.world,
        &mut buildings.0,
    ) {
        Ok(scene) => info!(
            "Scène de démo construite : {} bâtiments, {} convoyeurs, {} modules",
            scene.buildings, scene.conveyors, scene.modules
        ),
        Err(error) => warn!("Scène de démo impossible : {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chevrons_are_centred_and_stay_away_from_the_ends() {
        assert!(chevron_offsets(0.8).is_empty());
        assert_eq!(chevron_offsets(1.2), vec![0.0]);
        let offsets = chevron_offsets(10.0);
        assert_eq!(offsets.len(), 7);
        assert!((offsets[0] + offsets[6]).abs() < 1e-5, "centrées");
        assert!(offsets.iter().all(|z| z.abs() <= 10.0 / 2.0 - 0.5));
    }

    #[test]
    fn display_loads_near_and_unloads_far_with_hysteresis() {
        // Distances par défaut des options : 150 m pour charger, 170 m pour décharger.
        let defaults = Settings::default();
        let (load, unload) = (defaults.view_distance_m as f32, defaults.view_unload_m());
        assert_eq!(visibility_change(100.0, false, load, unload), Some(true));
        assert_eq!(
            visibility_change(160.0, false, load, unload),
            None,
            "entre les deux : pas encore"
        );
        assert_eq!(
            visibility_change(160.0, true, load, unload),
            None,
            "entre les deux : on garde"
        );
        assert_eq!(visibility_change(200.0, true, load, unload), Some(false));
        assert_eq!(visibility_change(100.0, true, load, unload), None);
    }
}
