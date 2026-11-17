//! Overlay de debug (F3), outils de test (T, I, O) et captures (F12).

pub mod capture;

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

use crate::app::PlayState;
use crate::building::{BuildMode, StreamStats};
use crate::game::{Screen, WorldSetup};
use crate::interaction::{Aim, Target};
use crate::inventory::ItemKind;
use crate::player::{EYE_HEIGHT, MoveMode, Player, PlayerInventory};
use crate::settings::Settings;
use crate::simulation::Simulation;
use crate::simulation::clock::TICK_RATE_HZ;
use crate::simulation::production::{MachineState, extractor_rate_per_min};
use crate::simulation::sim_world::{BuildingKind, Machine};
use crate::world::deposits::ResourceKind;
use crate::world::generation::WORLDGEN_VERSION;
use crate::world::{Deposits, Rocks, Terrain};

/// Minerais de chaque type donnés par l'outil « G ».
const GIVE_ITEMS: u32 = 50;

/// Objets ajoutés par l'outil « I » dans la machine visée.
const TEST_ITEMS: u32 = 10;

#[derive(Component)]
struct DebugOverlay;

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            capture::CapturePlugin,
        ))
        .add_systems(OnEnter(Screen::InGame), spawn_ui.in_set(WorldSetup::Spawn))
        .add_systems(
            Update,
            (toggle_overlay, update_overlay).run_if(in_state(Screen::InGame)),
        )
        .add_systems(Update, test_tools.run_if(in_state(PlayState::Running)));
    }
}

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        DebugOverlay,
        Text::new(""),
        TextFont::from_font_size(15.0),
        TextColor(Color::WHITE),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(8.0),
            padding: UiRect::all(Val::Px(8.0)),
            ..default()
        },
    ));
}

fn toggle_overlay(
    keys: Res<ButtonInput<KeyCode>>,
    mut overlay: Single<&mut Visibility, With<DebugOverlay>>,
) {
    if keys.just_pressed(KeyCode::F3) {
        overlay.toggle_visible_hidden();
    }
}

/// Outils de test : T change la vitesse du temps ; G donne des minerais ; I remplit la
/// machine visée (sortie de l'extracteur, entrée du four) et O vide sa sortie.
fn test_tools(
    keys: Res<ButtonInput<KeyCode>>,
    target: Res<Target>,
    mut sim: ResMut<Simulation>,
    mut inventory: ResMut<PlayerInventory>,
) {
    if keys.just_pressed(KeyCode::KeyT) {
        sim.cycle_speed();
        info!("Vitesse de simulation ×{}", sim.speed);
    }
    if keys.just_pressed(KeyCode::KeyG) {
        let iron = inventory.0.add(ItemKind::IronOre, GIVE_ITEMS);
        let copper = inventory.0.add(ItemKind::CopperOre, GIVE_ITEMS);
        info!("Outil de test : +{iron} minerais de fer, +{copper} de cuivre dans l'inventaire");
    }
    let Some(id) = target.0.building() else {
        return;
    };
    let Some(building) = sim.world.building_mut(id) else {
        return;
    };
    let name = building.machine.kind().name();
    if keys.just_pressed(KeyCode::KeyI) {
        let added = match &mut building.machine {
            Machine::Extractor(e) => e.output_mut().add(TEST_ITEMS),
            Machine::Furnace(f) => f.insert(ItemKind::IronOre, TEST_ITEMS),
            _ => 0,
        };
        info!("Outil de test : +{added} dans {name} #{}", id.0);
    }
    if keys.just_pressed(KeyCode::KeyO) {
        let removed = match &mut building.machine {
            Machine::Extractor(e) => e.output_mut().take_all(),
            Machine::Furnace(f) => f.output_mut().take(u32::MAX),
            _ => 0,
        };
        info!(
            "Outil de test : sortie de {name} #{} vidée ({removed})",
            id.0
        );
    }
}

fn update_overlay(
    diagnostics: Res<DiagnosticsStore>,
    sim: Res<Simulation>,
    state: Res<State<PlayState>>,
    terrain: Res<Terrain>,
    rocks: Res<Rocks>,
    deposits: Res<Deposits>,
    target: Res<Target>,
    build: Res<BuildMode>,
    stream: Res<StreamStats>,
    settings: Res<Settings>,
    player: Single<(&Transform, &Player)>,
    mut overlay: Single<&mut Text, With<DebugOverlay>>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|fps| fps.smoothed())
        .map_or_else(|| "—".to_string(), |fps| format!("{fps:.0}"));
    let (transform, player) = *player;
    let p = transform.translation;
    let field = &terrain.0;
    let world = &sim.world;
    let now = world.clock().tick();
    let ground = field.height_at(p.x, p.z);
    let mode = match player.mode {
        MoveMode::Walk if player.grounded => "marche (au sol)",
        MoveMode::Walk => "marche (en l'air)",
        MoveMode::Fly => "vol",
    };
    let v = player.velocity;
    let mut lines = vec![
        "Project Ascension — debug (F3)".to_string(),
        format!("FPS : {fps}"),
        format!(
            "Monde : seed {} · {} × {} m · gen v{}",
            field.seed().0,
            field.config().size_m,
            field.config().size_m,
            WORLDGEN_VERSION.0
        ),
        format!("Position : x {:.1} · y {:.1} · z {:.1} m", p.x, p.y, p.z),
        format!(
            "Sol : {ground:.1} m (pieds à {:.1} m) · {}",
            p.y - EYE_HEIGHT,
            if ground < 0.0 { "sous la mer" } else { "terre" }
        ),
        format!(
            "Vitesse : {:.1} m/s ({:.0} km/h) · horizontale {:.1} · verticale {:+.1}",
            v.length(),
            v.length() * 3.6,
            v.with_y(0.0).length(),
            v.y
        ),
        format!(
            "Mode : {mode} — V pour changer · construction {} (B)",
            if build.active { "ON" } else { "off" }
        ),
        format!("Visée : {}", describe_aim(target.0, &sim, &deposits, now)),
    ];

    let count = |kind| deposits.0.all().iter().filter(|d| d.kind == kind).count();
    let starter = deposits
        .0
        .starter()
        .and_then(|id| deposits.0.get(id))
        .map_or_else(
            || "aucun".to_string(),
            |d| {
                let (dx, dz) = (d.x - p.x, d.z - p.z);
                format!(
                    "{} à {:.0} m vers {}",
                    d.kind.name(),
                    dx.hypot(dz),
                    compass(dx, dz)
                )
            },
        );
    lines.push(format!(
        "Gisements : {} fer, {} cuivre · départ : {starter}",
        count(ResourceKind::Iron),
        count(ResourceKind::Copper)
    ));

    let mut counts = [0usize; BuildingKind::ALL.len()];
    let (mut active, mut stopped) = (0, 0);
    for (_, b) in world.buildings() {
        let kind = b.machine.kind();
        counts[BuildingKind::ALL
            .iter()
            .position(|k| *k == kind)
            .unwrap_or(0)] += 1;
        let state = match &b.machine {
            Machine::Extractor(e) => Some(e.state()),
            Machine::Furnace(f) => Some(f.state()),
            _ => None,
        };
        match state {
            Some(MachineState::Active) => active += 1,
            Some(_) => stopped += 1,
            None => {}
        }
    }
    let (mut theoretical, mut measured) = (0.0, 0.0);
    for (_, e) in world.extractors() {
        theoretical += e.effective_rate_per_min();
        measured += e.measured_rate_per_min(now);
    }
    let in_transit: usize = world.conveyors().map(|(_, c)| c.len()).sum();
    lines.push(format!(
        "Bâtiments : {} extracteur(s), {} générateur(s), {} four(s), {} coffre(s), {} poteau(x), {} hub(s), {} convoyeur(s) ({in_transit} objets en route) · {} module(s)",
        counts[0],
        counts[1],
        counts[2],
        counts[3],
        counts[4],
        counts[5],
        world.conveyors().count(),
        world.modules().count()
    ));
    lines.push(format!(
        "Machines : {active} actives, {stopped} à l'arrêt · extraction {theoretical:.1}/min théorique, {measured:.1}/min réel"
    ));
    // Réseaux qui contiennent au moins un générateur ou plusieurs bâtiments.
    let networks: Vec<String> = world
        .grid()
        .networks()
        .iter()
        .enumerate()
        .filter(|(_, n)| n.members.len() > 1 || n.produced_w > 0)
        .map(|(i, n)| {
            format!(
                "#{i} : {}/{} kW ({} %, {} bât.)",
                n.produced_w / 1000,
                n.demanded_w / 1000,
                n.satisfaction / 10,
                n.members.len()
            )
        })
        .collect();
    lines.push(if networks.is_empty() {
        "Réseaux : aucun (pose un générateur et un poteau)".to_string()
    } else {
        format!("Réseaux : {}", networks.join(" · "))
    });
    lines.push(format!(
        "Affichage : {}/{} bâtiments chargés (rayon {} m) · simulation : {} bâtiments, tous actifs",
        stream.loaded,
        stream.total,
        settings.view_distance_m,
        world.buildings().count()
    ));
    lines.push(format!(
        "Rochers : {} / {} ({} détruits)",
        rocks.0.live_count(),
        rocks.0.all().len(),
        rocks.0.destroyed().len()
    ));
    lines.push(format!(
        "État : {} · Temps simulé : {} (tick {now} @ {TICK_RATE_HZ} Hz) · ×{} — T pour changer",
        match state.get() {
            PlayState::Running => "en jeu",
            PlayState::Paused => "pause",
            PlayState::Menu => "menu ouvert",
        },
        world.clock().format_elapsed(),
        sim.speed
    ));
    // Charge CPU = coût d'un tick × ticks simulés par seconde (vitesse comprise).
    let ticks_per_second = (TICK_RATE_HZ * sim.speed) as f32;
    lines.push(format!(
        "Simulation : {:.1} µs par tick · charge CPU {:.3} % à ×{}",
        sim.tick_cost_us,
        sim.tick_cost_us * ticks_per_second / 1e6 * 100.0,
        sim.speed
    ));

    overlay.0 = lines.join("\n");
}

fn describe_aim(aim: Aim, sim: &Simulation, deposits: &Deposits, now: u64) -> String {
    match aim {
        Aim::Nothing => "rien".to_string(),
        Aim::Rock { id, distance } => {
            format!(
                "rocher #{} à {distance:.1} m — clic gauche pour détruire",
                id.0
            )
        }
        Aim::Building { id, distance } => {
            let world = &sim.world;
            let power = world.network_stats(id).map_or(0, |n| n.satisfaction / 10);
            match world.building(id).map(|b| &b.machine) {
                Some(Machine::Extractor(e)) => format!(
                    "extracteur #{} à {distance:.1} m · {} · alim. {power} % · {:.1}/min réel ({:.1} théorique) · sortie {}/{} — I : +{TEST_ITEMS} · O : vider · E : ouvrir",
                    id.0,
                    e.state().name(),
                    e.measured_rate_per_min(now),
                    e.effective_rate_per_min(),
                    e.output().count(),
                    e.output().capacity(),
                ),
                Some(Machine::Furnace(f)) => format!(
                    "four #{} à {distance:.1} m · {} · alim. {power} % · entrée {} · sortie {} — I : +{TEST_ITEMS} minerais · O : vider · E : ouvrir",
                    id.0,
                    f.state().name(),
                    f.input().count(),
                    f.output().count(),
                ),
                Some(m) => format!(
                    "{} #{} à {distance:.1} m · réseau {power} % — E : ouvrir",
                    m.kind().name().to_lowercase(),
                    id.0
                ),
                None => format!("bâtiment #{} à {distance:.1} m", id.0),
            }
        }
        Aim::Module { id, distance } => match sim.world.module(id) {
            Some(m) => {
                let stats = sim.world.stats_of(m.host);
                format!(
                    "{} #{} à {distance:.1} m sur #{} (emplacement {}) · machine : vitesse {} %, consommation {} %",
                    m.kind.name().to_lowercase(),
                    id.0,
                    m.host.0,
                    m.socket + 1,
                    stats.speed_percent,
                    stats.power_percent
                )
            }
            None => format!("module #{} à {distance:.1} m", id.0),
        },
        Aim::Conveyor { id, distance } => match sim.world.conveyor(id) {
            Some(c) => format!(
                "convoyeur #{} à {distance:.1} m · de #{} vers #{} · {}/{} objets · {} livrés · trajet {:.1} s",
                id.0,
                c.from.0,
                c.to.0,
                c.len(),
                c.capacity(),
                c.delivered_total(),
                c.travel_ticks() as f32 / TICK_RATE_HZ as f32
            ),
            None => format!("convoyeur #{} à {distance:.1} m", id.0),
        },
        Aim::Ground {
            distance, deposit, ..
        } => match deposit.and_then(|id| deposits.0.get(id)) {
            Some(d) => format!(
                "gisement de {} {} à {distance:.1} m · {}/min avec un extracteur · réserve infinie{}",
                d.kind.name(),
                d.purity.name(),
                extractor_rate_per_min(d.purity),
                if sim.world.extractor_on(d.id).is_some() {
                    " · déjà équipé"
                } else {
                    ""
                }
            ),
            None => format!("sol à {distance:.1} m"),
        },
    }
}

/// Direction cardinale approximative d'un vecteur horizontal (nord = -Z).
fn compass(dx: f32, dz: f32) -> &'static str {
    // Angle depuis le nord, dans le sens horaire vu du dessus (est = +X).
    let angle = dx.atan2(-dz).to_degrees().rem_euclid(360.0);
    const NAMES: [&str; 8] = ["N", "N-E", "E", "S-E", "S", "S-O", "O", "N-O"];
    NAMES[((angle + 22.5) / 45.0) as usize % 8]
}

#[cfg(test)]
mod tests {
    use super::compass;

    #[test]
    fn compass_points_the_right_way() {
        assert_eq!(compass(0.0, -10.0), "N");
        assert_eq!(compass(10.0, 0.0), "E");
        assert_eq!(compass(0.0, 10.0), "S");
        assert_eq!(compass(-10.0, 0.0), "O");
        assert_eq!(compass(5.0, -5.0), "N-E");
    }
}
