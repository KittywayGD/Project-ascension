//! Interaction avec le monde : réticule et visée unique (terrain, rocher, bâtiment, convoyeur),
//! plus la destruction des rochers (outil de test V0, hors mode construction).

use bevy::prelude::*;

use crate::app::PlayState;
use crate::building::{BuildMode, Buildings};
use crate::player::Player;
use crate::simulation::Simulation;
use crate::simulation::sim_world::BuildingId;
use crate::world::deposits::DepositId;
use crate::world::rocks::RockId;
use crate::world::{Deposits, RockEntities, RockMaterials, Rocks, Terrain};

/// Portée de la visée, en mètres.
pub const REACH_M: f32 = 12.0;

/// Ce que le joueur vise : l'élément le plus proche le long du rayon de la caméra.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Aim {
    #[default]
    Nothing,
    Rock {
        id: RockId,
        distance: f32,
    },
    Building {
        id: BuildingId,
        distance: f32,
    },
    Conveyor {
        id: BuildingId,
        distance: f32,
    },
    Ground {
        point: Vec3,
        distance: f32,
        /// Gisement dont la zone contient le point visé.
        deposit: Option<DepositId>,
    },
}

impl Aim {
    pub fn rock(self) -> Option<RockId> {
        match self {
            Aim::Rock { id, .. } => Some(id),
            _ => None,
        }
    }

    pub fn building(self) -> Option<BuildingId> {
        match self {
            Aim::Building { id, .. } => Some(id),
            _ => None,
        }
    }
}

/// Visée courante, mise à jour chaque frame.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct Target(pub Aim);

pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Target>()
            .add_systems(Startup, spawn_crosshair)
            .add_systems(
                Update,
                (update_target, highlight_rock, destroy_rock)
                    .chain()
                    .run_if(in_state(PlayState::Running)),
            );
    }
}

fn spawn_crosshair(mut commands: Commands) {
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_child((
            Text::new("+"),
            TextFont::from_font_size(24.0),
            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
        ));
}

pub fn update_target(
    camera: Single<&Transform, With<Player>>,
    terrain: Res<Terrain>,
    rocks: Res<Rocks>,
    deposits: Res<Deposits>,
    buildings: Res<Buildings>,
    sim: Res<Simulation>,
    mut target: ResMut<Target>,
) {
    let origin = camera.translation;
    let direction = camera.forward().as_vec3();

    let mut aim = Aim::Nothing;
    let mut nearest = REACH_M;
    if let Some(distance) = terrain.0.raycast(origin, direction, nearest) {
        let point = origin + direction * distance;
        let deposit = deposits.0.find_at(point.x, point.z).map(|d| d.id);
        aim = Aim::Ground {
            point,
            distance,
            deposit,
        };
        nearest = distance;
    }
    // Rochers et bâtiments ne gagnent que s'ils sont devant le terrain.
    if let Some((id, distance)) = rocks.0.raycast(origin, direction, nearest) {
        aim = Aim::Rock { id, distance };
        nearest = distance;
    }
    if let Some((id, distance)) = buildings.0.raycast(origin, direction, nearest) {
        aim = Aim::Building { id, distance };
        nearest = distance;
    }
    if let Some((id, distance)) = buildings
        .0
        .raycast_conveyors(&sim.world, origin, direction, nearest)
    {
        aim = Aim::Conveyor { id, distance };
    }
    // N'écrit que si ça change, pour que `is_changed` reste significatif.
    target.set_if_neq(Target(aim));
}

/// Surligne le rocher visé ; remet l'ancien à son matériau normal.
fn highlight_rock(
    target: Res<Target>,
    mut previous: Local<Option<RockId>>,
    entities: Res<RockEntities>,
    materials: Res<RockMaterials>,
    mut rock_materials: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    let current = target.0.rock();
    if current == *previous {
        return;
    }
    let mut set = |id: RockId, handle: &Handle<StandardMaterial>| {
        if let Some(mut material) = entities
            .0
            .get(&id)
            .and_then(|entity| rock_materials.get_mut(*entity).ok())
        {
            material.0 = handle.clone();
        }
    };
    if let Some(old) = *previous {
        set(old, &materials.normal);
    }
    if let Some(new) = current {
        set(new, &materials.highlighted);
    }
    *previous = current;
}

fn destroy_rock(
    mouse: Res<ButtonInput<MouseButton>>,
    build: Res<BuildMode>,
    mut commands: Commands,
    mut target: ResMut<Target>,
    mut rocks: ResMut<Rocks>,
    mut entities: ResMut<RockEntities>,
) {
    // En mode construction, le clic gauche pose un bâtiment.
    if build.active || !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(id) = target.0.rock() else {
        return;
    };
    // D'abord la donnée de jeu, ensuite l'affichage : le terrain n'est jamais touché.
    if rocks.0.destroy(id) {
        if let Some(entity) = entities.0.remove(&id) {
            commands.entity(entity).despawn();
        }
        target.0 = Aim::Nothing;
        info!(
            "Rocher #{} détruit ({} restants)",
            id.0,
            rocks.0.live_count()
        );
    }
}
