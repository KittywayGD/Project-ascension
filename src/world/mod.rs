//! Monde : génération déterministe (Rust pur) et présentation Bevy (chunks, eau, lumière).

pub mod deposits;
pub mod generation;
pub mod noise;
pub mod rocks;
pub mod terrain;

use std::collections::HashMap;
use std::time::Instant;

use bevy::prelude::*;

use deposits::{DepositConfig, DepositField, ResourceKind};
use generation::{HeightField, WORLDGEN_VERSION, WorldGenConfig, WorldSeed};
use noise::SplitMix64;
use rocks::{RockConfig, RockField, RockId};
use terrain::{TerrainChunk, build_chunk_mesh, chunk_count, chunk_origin};

/// Paramètres du monde choisis au lancement (seed en argument, configuration par défaut).
#[derive(Resource, Debug, Clone)]
pub struct WorldParams {
    pub seed: WorldSeed,
    pub config: WorldGenConfig,
}

/// Heightmap du monde courant, partagée par le rendu et le déplacement du joueur.
#[derive(Resource, Debug)]
pub struct Terrain(pub HeightField);

/// Rochers destructibles du monde courant (données de jeu, indépendantes des entités).
#[derive(Resource, Debug)]
pub struct Rocks(pub RockField);

/// Gisements du monde courant (réserves infinies).
#[derive(Resource, Debug)]
pub struct Deposits(pub DepositField);

/// Correspondance rocher → entité affichée (les entités ne sont pas des identifiants stables).
#[derive(Resource, Debug, Default)]
pub struct RockEntities(pub HashMap<RockId, Entity>);

/// Matériaux des rochers : normal et surligné quand il est visé.
#[derive(Resource, Debug)]
pub struct RockMaterials {
    pub normal: Handle<StandardMaterial>,
    pub highlighted: Handle<StandardMaterial>,
}

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(SKY_COLOR))
            .insert_resource(GlobalAmbientLight {
                color: Color::WHITE,
                brightness: 400.0,
                ..default()
            })
            // PreStartup : la heightmap existe avant que le joueur ne cherche son point d'apparition.
            .add_systems(PreStartup, generate_world)
            .add_systems(
                Startup,
                (
                    spawn_terrain_chunks,
                    spawn_rocks,
                    spawn_deposits,
                    spawn_water,
                    spawn_sun,
                ),
            );
    }
}

/// Couleur du ciel, réutilisée par le brouillard de la caméra.
pub const SKY_COLOR: Color = Color::srgb(0.62, 0.76, 0.90);

fn generate_world(mut commands: Commands, params: Res<WorldParams>) {
    let start = Instant::now();
    let field = HeightField::generate(params.seed, &params.config);
    let rocks = RockField::generate(params.seed, &field, &RockConfig::default());
    let deposits = DepositField::generate(params.seed, &field, &rocks, &DepositConfig::default());
    info!(
        "Monde généré : seed {} · {} × {} m · gen v{} · empreinte {:016x} · {} rochers (empreinte {:016x}) · {:.0} ms",
        params.seed.0,
        params.config.size_m,
        params.config.size_m,
        WORLDGEN_VERSION.0,
        field.checksum(),
        rocks.all().len(),
        rocks.checksum(),
        start.elapsed().as_secs_f64() * 1000.0
    );
    let count = |kind| deposits.all().iter().filter(|d| d.kind == kind).count();
    match deposits.check_quotas() {
        Ok(()) => info!(
            "Gisements : {} fer, {} cuivre (empreinte {:016x}) · quotas OK",
            count(ResourceKind::Iron),
            count(ResourceKind::Copper),
            deposits.checksum()
        ),
        Err(error) => warn!("Quotas de gisements non respectés : {error}"),
    }
    commands.insert_resource(Terrain(field));
    commands.insert_resource(Rocks(rocks));
    commands.insert_resource(Deposits(deposits));
}

/// Chaque gisement est un amas de pépites de minerai, colorées selon la ressource.
/// Purement visuel : pas d'obstacle, la visée passe par la zone du gisement.
fn spawn_deposits(
    mut commands: Commands,
    terrain: Res<Terrain>,
    deposits: Res<Deposits>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let nugget = meshes.add(
        Sphere::new(1.0)
            .mesh()
            .ico(0)
            .expect("icosphère valide")
            .with_duplicated_vertices()
            .with_computed_flat_normals(),
    );
    let mut ore = |color: Color, glow: LinearRgba| {
        materials.add(StandardMaterial {
            base_color: color,
            emissive: glow,
            metallic: 0.4,
            perceptual_roughness: 0.5,
            ..default()
        })
    };
    let iron = ore(
        Color::srgb(0.55, 0.25, 0.16),
        LinearRgba::rgb(0.12, 0.03, 0.01),
    );
    let copper = ore(
        Color::srgb(0.85, 0.50, 0.22),
        LinearRgba::rgb(0.15, 0.07, 0.02),
    );

    for deposit in deposits.0.all() {
        let material = match deposit.kind {
            ResourceKind::Iron => iron.clone(),
            ResourceKind::Copper => copper.clone(),
        };
        // Aspect seulement (pas la génération) : graine dérivée de l'identifiant.
        let mut rng = SplitMix64::new(u64::from(deposit.id.0) ^ 0x0BE5);
        let count = 6 + deposit.purity.rate_percent() / 25; // plus pur = plus de pépites
        for _ in 0..count {
            let angle = rng.next_f64() as f32 * std::f32::consts::TAU;
            let distance = rng.next_f64() as f32 * deposit.radius * 0.85;
            let size = 0.25 + rng.next_f64() as f32 * 0.4;
            let (x, z) = (
                deposit.x + angle.cos() * distance,
                deposit.z + angle.sin() * distance,
            );
            let y = terrain.0.height_at(x, z) - size * 0.3;
            commands.spawn((
                Name::new(format!("Gisement #{}", deposit.id.0)),
                Mesh3d(nugget.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_xyz(x, y, z)
                    .with_rotation(Quat::from_rotation_y(angle))
                    .with_scale(Vec3::new(size, size * 0.7, size)),
            ));
        }
    }
}

fn spawn_rocks(
    mut commands: Commands,
    rocks: Res<Rocks>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Icosphère peu subdivisée + normales plates : aspect taillé, un seul maillage partagé.
    let mesh = Sphere::new(1.0)
        .mesh()
        .ico(1)
        .expect("icosphère valide")
        .with_duplicated_vertices()
        .with_computed_flat_normals();
    let mesh = meshes.add(mesh);
    let normal = materials.add(StandardMaterial {
        base_color: Color::srgb(0.52, 0.50, 0.47),
        perceptual_roughness: 0.9,
        ..default()
    });
    let highlighted = materials.add(StandardMaterial {
        base_color: Color::srgb(0.62, 0.58, 0.50),
        emissive: LinearRgba::rgb(0.25, 0.18, 0.05),
        perceptual_roughness: 0.9,
        ..default()
    });

    let mut entities = RockEntities::default();
    for rock in rocks.0.live() {
        let entity = commands
            .spawn((
                Name::new(format!("Rocher #{}", rock.id.0)),
                Mesh3d(mesh.clone()),
                MeshMaterial3d(normal.clone()),
                Transform::from_translation(rock.center())
                    .with_rotation(Quat::from_rotation_y(rock.yaw))
                    .with_scale(rock.half_extents()),
            ))
            .id();
        entities.0.insert(rock.id, entity);
    }
    commands.insert_resource(entities);
    commands.insert_resource(RockMaterials {
        normal,
        highlighted,
    });
}

fn spawn_terrain_chunks(
    mut commands: Commands,
    terrain: Res<Terrain>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let start = Instant::now();
    let field = &terrain.0;
    // Couleur blanche : la teinte vient des couleurs de sommets (altitude, pente).
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.95,
        ..default()
    });

    let count = chunk_count(field);
    for z in 0..count {
        for x in 0..count {
            let chunk = TerrainChunk { x, z };
            commands.spawn((
                Name::new(format!("Chunk ({x}, {z})")),
                chunk,
                Mesh3d(meshes.add(build_chunk_mesh(field, chunk))),
                MeshMaterial3d(material.clone()),
                Transform::from_translation(chunk_origin(field, chunk)),
            ));
        }
    }
    info!(
        "{} chunks de terrain maillés en {:.0} ms",
        count * count,
        start.elapsed().as_secs_f64() * 1000.0
    );
}

/// Mer simplifiée au niveau y = 0, plus grande que la carte pour former l'horizon.
fn spawn_water(
    mut commands: Commands,
    terrain: Res<Terrain>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let size = terrain.0.config().size_m as f32 * 4.0;
    commands.spawn((
        Name::new("Mer"),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(size, size))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.12, 0.32, 0.50, 0.82),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.15,
            ..default()
        })),
    ));
}

fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        Name::new("Soleil"),
        DirectionalLight {
            illuminance: 10_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(30.0, 60.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
