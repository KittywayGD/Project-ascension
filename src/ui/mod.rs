//! Menus en jeu : inventaire (Tab), panneau de machine (E en visant un bâtiment) et
//! menu de construction (B).
//!
//! Menu ouvert, la souris est libre et le joueur ne bouge plus, mais la simulation
//! continue : on voit les machines travailler en direct. Les cases se manipulent au
//! **glisser-déposer**. Une aide contextuelle s'affiche aussi sous le réticule.

mod screens;

pub use screens::ScreensPlugin;

use bevy::picking::Pickable;
use bevy::picking::events::{PointerDragDrop, PointerDragEnd, PointerDragStart};
use bevy::picking::hover::Hovered;
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};
use bevy::window::PrimaryWindow;

use crate::app::PlayState;
use crate::building::{BuildMode, BuildTool};
use crate::game::GameSession;
use crate::game::{Screen, WorldSetup, reset};
use crate::interaction::{Aim, Target};
use crate::inventory::{Inventory, ItemKind, move_between, take_from_slot, transfer};
use crate::player::{INVENTORY_SLOTS, PlayerInventory};
use crate::simulation::Simulation;
use crate::simulation::energy::{GENERATOR_POWER_W, POLE_RANGE_M};
use crate::simulation::logistics::conveyor_max_rate_per_min;
use crate::simulation::modules::{MachineStats, Module};
use crate::simulation::sim_world::{BuildingId, BuildingKind, Machine, STORAGE_SLOTS, SimWorld};
use crate::simulation::tech::{TIERS, Tier, tier_reached};
use crate::world::Deposits;

/// Police de l'interface : Fira Sans complète (accents, symboles), embarquée dans le
/// binaire. La police par défaut de Bevy n'est qu'un sous-ensemble ASCII de Fira Mono :
/// « électrique » s'y affichait « �lectrique ».
const UI_FONT: &[u8] = include_bytes!("../../assets/fonts/FiraSans-Regular.ttf");

/// Ombre portée discrète sous les textes posés sur la scène 3D.
pub fn text_shadow() -> TextShadow {
    TextShadow {
        offset: Vec2::splat(1.5),
        color: Color::srgba(0.0, 0.0, 0.0, 0.8),
    }
}

/// Remplace la police par défaut (même identifiant d'asset) : tous les textes en héritent.
fn install_ui_font(mut fonts: ResMut<Assets<Font>>) {
    if let Err(error) = fonts.insert(AssetId::default(), Font::from_bytes(UI_FONT.to_vec())) {
        warn!("Police de l'interface non installée : {error:?}");
    }
}

const COLUMNS: usize = 9;
const SLOT_PX: f32 = 52.0;

const PANEL_BG: Color = Color::srgba(0.08, 0.09, 0.11, 0.96);
const SLOT_EMPTY: Color = Color::srgb(0.16, 0.17, 0.20);
const SLOT_BORDER: Color = Color::srgb(0.30, 0.31, 0.35);
const BUTTON_IDLE: Color = Color::srgb(0.22, 0.42, 0.28);
const BUTTON_HOVER: Color = Color::srgb(0.30, 0.56, 0.37);
const CARD_IDLE: Color = Color::srgb(0.14, 0.16, 0.20);
const CARD_HOVER: Color = Color::srgb(0.20, 0.24, 0.30);
const ACCENT: Color = Color::srgb(0.55, 0.85, 0.60);
const MUTED: Color = Color::srgb(0.70, 0.72, 0.76);
const LOCKED_CARD: Color = Color::srgb(0.10, 0.11, 0.13);
const LOCKED_TEXT: Color = Color::srgb(0.95, 0.65, 0.45);
const PRICE_TEXT: Color = Color::srgb(0.95, 0.85, 0.55);

/// Première lettre en majuscule.
fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Menu actuellement ouvert.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum OpenMenu {
    #[default]
    None,
    Inventory,
    Machine(BuildingId),
    Build,
}

/// Dernier message d'action affiché dans le panneau (« 12 récupérés », « inventaire plein »…).
#[derive(Resource, Debug, Default)]
struct PanelMessage(String);

/// Case en cours de glisser-déposer (pour afficher l'objet sous la souris).
#[derive(Resource, Debug, Default)]
struct Dragging(Option<SlotRef>);

#[derive(Component)]
struct MenuRoot;

/// Case affichée : inventaire du joueur, case de coffre, ou entrée / sortie de la machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotRef {
    Inventory(usize),
    Storage(usize),
    Input,
    Output,
}

#[derive(Component)]
struct SlotBox(SlotRef);

#[derive(Component)]
struct SlotText(SlotRef);

#[derive(Component)]
struct MachineInfo;

#[derive(Component)]
struct ProgressFill;

#[derive(Component)]
struct MessageText;

#[derive(Component)]
struct InventoryTitle;

/// Icône qui suit la souris pendant un glisser-déposer.
#[derive(Component)]
struct DragGhost;

#[derive(Component)]
struct DragGhostText;

/// Actions des boutons des menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    TakeAll,
    StoreAll,
    Deposit,
    Toggle,
    Select(BuildTool),
    StopBuilding,
    RemoveModule(BuildingId),
    DeliverAll,
}

#[derive(Component)]
struct PanelButton(Action);

#[derive(Component)]
struct ToggleLabel;

/// Ligne « Modules » du panneau de machine.
#[derive(Component)]
struct ModulesInfo;

/// Progression du palier suivant, en haut à droite.
#[derive(Component)]
struct GoalTracker;

/// Panneau d'aide (H) : contrôles, légende des couleurs, outils de test.
#[derive(Component)]
struct HelpPanel;

/// Notification temporaire (palier débloqué).
#[derive(Component)]
struct Toast;

/// Fin d'affichage de la notification, en secondes depuis le lancement.
#[derive(Resource, Debug, Default)]
struct ToastUntil(f32);

/// Demande de reconstruction du menu ouvert (après un retrait de module).
#[derive(Resource, Debug, Default)]
struct RebuildMenu(bool);

/// Carte du menu de construction (fond différent des boutons).
#[derive(Component)]
pub(crate) struct Card;

#[derive(Component)]
struct Prompt;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OpenMenu>()
            .init_resource::<PanelMessage>()
            .init_resource::<Dragging>()
            .init_resource::<RebuildMenu>()
            .init_resource::<ToastUntil>()
            .add_systems(PreStartup, install_ui_font)
            .add_systems(
                OnEnter(Screen::InGame),
                (
                    spawn_prompt,
                    spawn_goal_hud,
                    spawn_help,
                    init_announced_tier,
                )
                    .in_set(WorldSetup::Spawn),
            )
            .add_systems(
                OnExit(Screen::InGame),
                (
                    reset::<OpenMenu>,
                    reset::<PanelMessage>,
                    reset::<Dragging>,
                    reset::<RebuildMenu>,
                    reset::<ToastUntil>,
                ),
            )
            .add_systems(
                Update,
                (update_goal_hud, toggle_help, update_prompt).run_if(in_state(Screen::InGame)),
            )
            .add_systems(Update, open_menu.run_if(in_state(PlayState::Running)))
            .add_systems(
                Update,
                (close_menu, rebuild_menu, update_menu, follow_cursor)
                    .chain()
                    .run_if(in_state(PlayState::Menu)),
            )
            .add_systems(
                Update,
                style_buttons.run_if(
                    in_state(PlayState::Menu)
                        .or_else(in_state(PlayState::Paused))
                        .or_else(in_state(Screen::Title)),
                ),
            )
            .add_systems(OnEnter(PlayState::Menu), spawn_menu)
            .add_systems(OnExit(PlayState::Menu), despawn_menu);
    }
}

fn open_menu(
    keys: Res<ButtonInput<KeyCode>>,
    target: Res<Target>,
    sim: Res<Simulation>,
    mut menu: ResMut<OpenMenu>,
    mut next: ResMut<NextState<PlayState>>,
) {
    let wanted = if keys.just_pressed(KeyCode::Tab) {
        Some(OpenMenu::Inventory)
    } else if keys.just_pressed(KeyCode::KeyB) {
        Some(OpenMenu::Build)
    } else if keys.just_pressed(KeyCode::KeyE) {
        // Viser un module ouvre la machine qui le porte.
        let machine = match target.0 {
            Aim::Module { id, .. } => sim.world.module(id).map(|m| m.host),
            aim => aim.building(),
        };
        machine
            .filter(|id| sim.world.building(*id).is_some())
            .map(OpenMenu::Machine)
    } else {
        None
    };
    if let Some(wanted) = wanted {
        *menu = wanted;
        next.set(PlayState::Menu);
    }
}

/// Tab, E ou B referment le menu (Échap aussi, géré par `app.rs`).
fn close_menu(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<PlayState>>) {
    if keys.any_just_pressed([KeyCode::Tab, KeyCode::KeyE, KeyCode::KeyB]) {
        next.set(PlayState::Running);
    }
}

fn despawn_menu(
    mut commands: Commands,
    roots: Query<Entity, With<MenuRoot>>,
    mut menu: ResMut<OpenMenu>,
    mut message: ResMut<PanelMessage>,
    mut dragging: ResMut<Dragging>,
) {
    for root in &roots {
        commands.entity(root).despawn();
    }
    *menu = OpenMenu::None;
    message.0.clear();
    dragging.0 = None;
}

fn text(content: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(content),
        TextFont::from_font_size(size),
        TextColor(color),
    )
}

/// Une case : fond coloré selon l'objet, symbole et quantité ; se glisse-dépose.
fn spawn_slot(parent: &mut ChildSpawnerCommands, slot: SlotRef) {
    parent
        .spawn((
            SlotBox(slot),
            Node {
                width: Val::Px(SLOT_PX),
                height: Val::Px(SLOT_PX),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(5.0)),
                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::FlexEnd,
                padding: UiRect::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(SLOT_EMPTY),
            BorderColor::all(SLOT_BORDER),
        ))
        .with_child((
            SlotText(slot),
            text("", 13.0, Color::WHITE),
            TextLayout::justify(Justify::Right),
            Pickable::IGNORE,
        ))
        .observe(on_drag_start)
        .observe(on_drag_end)
        .observe(on_drop);
}

fn spawn_button(parent: &mut ChildSpawnerCommands, action: Action, label: &str) {
    let mut button = parent.spawn((
        PanelButton(action),
        Button,
        Hovered::default(),
        Node {
            padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(BUTTON_IDLE),
    ));
    button.observe(on_button);
    button.with_children(|b| {
        let mut label = b.spawn((text(label, 15.0, Color::WHITE), Pickable::IGNORE));
        if action == Action::Toggle {
            label.insert(ToggleLabel);
        }
    });
}

fn progress_bar(parent: &mut ChildSpawnerCommands, width: Val) {
    parent
        .spawn((
            Node {
                width,
                height: Val::Px(10.0),
                border_radius: BorderRadius::all(Val::Px(5.0)),
                ..default()
            },
            BackgroundColor(SLOT_EMPTY),
        ))
        .with_child((
            ProgressFill,
            Node {
                width: Val::Percent(0.0),
                height: Val::Percent(100.0),
                border_radius: BorderRadius::all(Val::Px(5.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.35, 0.80, 0.45)),
        ));
}

fn row() -> Node {
    Node {
        align_items: AlignItems::Center,
        column_gap: Val::Px(12.0),
        ..default()
    }
}

fn slot_grid(parent: &mut ChildSpawnerCommands, slots: impl IntoIterator<Item = SlotRef>) {
    parent
        .spawn(Node {
            display: Display::Grid,
            grid_template_columns: RepeatedGridTrack::px(COLUMNS as u16, SLOT_PX),
            row_gap: Val::Px(4.0),
            column_gap: Val::Px(4.0),
            ..default()
        })
        .with_children(|grid| {
            for slot in slots {
                spawn_slot(grid, slot);
            }
        });
}

fn spawn_menu(
    mut commands: Commands,
    menu: Res<OpenMenu>,
    sim: Res<Simulation>,
    build: Res<BuildMode>,
    session: Res<GameSession>,
) {
    spawn_menu_ui(&mut commands, *menu, &sim, &build, session.creative);
}

/// Reconstruit le menu ouvert (la liste des modules a changé).
fn rebuild_menu(
    mut rebuild: ResMut<RebuildMenu>,
    roots: Query<Entity, With<MenuRoot>>,
    mut commands: Commands,
    menu: Res<OpenMenu>,
    sim: Res<Simulation>,
    build: Res<BuildMode>,
    session: Res<GameSession>,
) {
    if !std::mem::take(&mut rebuild.0) {
        return;
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    spawn_menu_ui(&mut commands, *menu, &sim, &build, session.creative);
}

fn spawn_menu_ui(
    commands: &mut Commands,
    menu: OpenMenu,
    sim: &Simulation,
    build: &BuildMode,
    creative: bool,
) {
    let kind = match menu {
        OpenMenu::Machine(id) => sim.world.building(id).map(|b| b.machine.kind()),
        _ => None,
    };
    let modules = match menu {
        OpenMenu::Machine(id) => sim.world.modules_of(id),
        _ => Vec::new(),
    };
    commands
        .spawn((
            MenuRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(18.0)),
                    row_gap: Val::Px(10.0),
                    border_radius: BorderRadius::all(Val::Px(10.0)),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
            ))
            .with_children(|panel| {
                if menu == OpenMenu::Build {
                    spawn_build_menu(panel, build, sim, creative);
                    return;
                }
                if let Some(kind) = kind {
                    spawn_machine_section(panel, kind, &modules);
                }
                panel.spawn((InventoryTitle, text("Inventaire", 18.0, Color::WHITE)));
                slot_grid(panel, (0..INVENTORY_SLOTS).map(SlotRef::Inventory));
                panel.spawn((MessageText, text("", 14.0, MUTED)));
                let close = if kind.is_some() {
                    "E ou Échap : fermer"
                } else {
                    "Tab ou Échap : fermer"
                };
                panel.spawn(text(
                    format!(
                        "Glisse une case vers une autre pour la déplacer · {close} · la simulation continue"
                    ),
                    13.0,
                    MUTED,
                ));
            });

            // Icône du glisser-déposer, au-dessus de tout, transparente aux clics.
            root.spawn((
                DragGhost,
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(SLOT_PX),
                    height: Val::Px(SLOT_PX),
                    justify_content: JustifyContent::FlexEnd,
                    align_items: AlignItems::FlexEnd,
                    padding: UiRect::all(Val::Px(4.0)),
                    border_radius: BorderRadius::all(Val::Px(5.0)),
                    ..default()
                },
                BackgroundColor(SLOT_EMPTY),
                GlobalZIndex(20),
                Pickable::IGNORE,
                Visibility::Hidden,
            ))
            .with_child((
                DragGhostText,
                text("", 13.0, Color::WHITE),
                TextLayout::justify(Justify::Right),
                Pickable::IGNORE,
            ));
        });
}

/// Menu de construction : une carte par outil, avec son prix ; les outils verrouillés
/// indiquent le palier à atteindre et ne se sélectionnent pas.
fn spawn_build_menu(
    panel: &mut ChildSpawnerCommands,
    build: &BuildMode,
    sim: &Simulation,
    creative: bool,
) {
    panel.spawn(text("Construction", 20.0, Color::WHITE));
    panel
        .spawn(Node {
            display: Display::Grid,
            grid_template_columns: RepeatedGridTrack::px(3, 230.0),
            row_gap: Val::Px(8.0),
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|grid| {
            for tool in BuildTool::ALL {
                let selected = build.active && build.tool == tool;
                let locked = tool.locked_reason(sim, creative);
                let mut card = grid.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(4.0),
                        padding: UiRect::all(Val::Px(10.0)),
                        border: UiRect::all(Val::Px(2.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(if locked.is_some() {
                        LOCKED_CARD
                    } else {
                        CARD_IDLE
                    }),
                    BorderColor::all(if selected { ACCENT } else { SLOT_BORDER }),
                ));
                if locked.is_none() {
                    card.insert((
                        PanelButton(Action::Select(tool)),
                        Card,
                        Button,
                        Hovered::default(),
                    ))
                    .observe(on_button);
                }
                card.with_children(|card| {
                    let name_color = if locked.is_some() {
                        MUTED
                    } else {
                        Color::WHITE
                    };
                    card.spawn((text(tool.name(), 17.0, name_color), Pickable::IGNORE));
                    match &locked {
                        Some(reason) => {
                            card.spawn((
                                text(capitalize(reason), 13.0, LOCKED_TEXT),
                                Pickable::IGNORE,
                            ));
                        }
                        None => {
                            card.spawn((text(tool.stats(), 14.0, ACCENT), Pickable::IGNORE));
                        }
                    }
                    let price = if creative {
                        "Gratuit (créatif)".to_string()
                    } else {
                        format!("Coût : {}", tool.cost_label())
                    };
                    card.spawn((text(price, 13.0, PRICE_TEXT), Pickable::IGNORE));
                    card.spawn((text(tool.description(), 13.0, MUTED), Pickable::IGNORE));
                });
            }
        });
    panel.spawn(row()).with_children(|r| {
        if build.active {
            spawn_button(r, Action::StopBuilding, "Arrêter de construire");
        }
        r.spawn(text(
            "Clic : choisir · B ou Échap : fermer · en construction, Échap arrête",
            13.0,
            MUTED,
        ));
    });
}

fn spawn_machine_section(
    panel: &mut ChildSpawnerCommands,
    kind: BuildingKind,
    modules: &[(BuildingId, Module)],
) {
    panel.spawn((MachineInfo, text("", 16.0, Color::WHITE)));
    if kind.module_slots() > 0 {
        panel.spawn(row()).with_children(|r| {
            r.spawn((ModulesInfo, text("", 15.0, ACCENT)));
            for (id, module) in modules {
                spawn_button(
                    r,
                    Action::RemoveModule(*id),
                    &format!("Retirer {}", module.kind.short_name()),
                );
            }
        });
    }
    match kind {
        BuildingKind::Extractor => {
            progress_bar(panel, Val::Percent(100.0));
            panel.spawn(row()).with_children(|r| {
                r.spawn(text("Sortie", 15.0, MUTED));
                spawn_slot(r, SlotRef::Output);
                spawn_button(r, Action::TakeAll, "Tout prendre");
                spawn_button(r, Action::Toggle, "Éteindre");
            });
        }
        BuildingKind::Furnace => {
            panel.spawn(row()).with_children(|r| {
                r.spawn(text("Entrée", 15.0, MUTED));
                spawn_slot(r, SlotRef::Input);
                progress_bar(r, Val::Px(160.0));
                spawn_slot(r, SlotRef::Output);
                r.spawn(text("Sortie", 15.0, MUTED));
            });
            panel.spawn(row()).with_children(|r| {
                spawn_button(r, Action::Deposit, "Déposer du minerai");
                spawn_button(r, Action::TakeAll, "Tout prendre");
                spawn_button(r, Action::Toggle, "Éteindre");
            });
        }
        BuildingKind::Storage => {
            slot_grid(panel, (0..STORAGE_SLOTS).map(SlotRef::Storage));
            panel.spawn(row()).with_children(|r| {
                spawn_button(r, Action::TakeAll, "Tout prendre");
                spawn_button(r, Action::StoreAll, "Tout ranger");
            });
        }
        BuildingKind::Generator => {
            panel.spawn(row()).with_children(|r| {
                spawn_button(r, Action::Toggle, "Éteindre");
            });
        }
        BuildingKind::Hub => {
            panel.spawn(row()).with_children(|r| {
                spawn_button(r, Action::DeliverAll, "Livrer tout l'inventaire");
            });
        }
        BuildingKind::Pole => {}
    }
    // Espace avant l'inventaire.
    panel.spawn(Node {
        height: Val::Px(6.0),
        ..default()
    });
}

/// Réagit à un clic sur un bouton ou une carte d'un menu.
fn on_button(
    activate: On<Activate>,
    buttons: Query<&PanelButton>,
    menu: Res<OpenMenu>,
    mut build: ResMut<BuildMode>,
    mut sim: ResMut<Simulation>,
    mut inventory: ResMut<PlayerInventory>,
    mut message: ResMut<PanelMessage>,
    mut rebuild: ResMut<RebuildMenu>,
    mut next: ResMut<NextState<PlayState>>,
) {
    let Ok(PanelButton(action)) = buttons.get(activate.entity) else {
        return;
    };
    match *action {
        Action::Select(tool) => {
            build.select(tool);
            info!("Outil de construction : {}", tool.name());
            next.set(PlayState::Running);
            return;
        }
        Action::StopBuilding => {
            build.stop();
            next.set(PlayState::Running);
            return;
        }
        Action::DeliverAll => {
            let OpenMenu::Machine(hub) = *menu else {
                return;
            };
            let mut delivered = 0;
            for kind in ItemKind::ALL {
                let count = inventory.0.count(kind);
                if count == 0 {
                    continue;
                }
                // D'abord retirer de l'inventaire, puis compter : rien n'est dupliqué.
                let removed = inventory.0.remove(kind, count);
                if sim.world.deliver_by_hand(hub, kind, u64::from(removed)) {
                    delivered += removed;
                } else {
                    inventory.0.add(kind, removed);
                }
            }
            message.0 = if delivered == 0 {
                "Rien à livrer : l'inventaire est vide.".to_string()
            } else {
                format!("{delivered} objet(s) livré(s) au hub.")
            };
            info!("{}", message.0);
            return;
        }
        Action::RemoveModule(id) => {
            if let Some(module) = sim.world.detach_module(id) {
                message.0 = format!(
                    "{} retiré : il disparaît du toit (contenu de la machine conservé).",
                    module.kind.name()
                );
                info!("{}", message.0);
            }
            rebuild.0 = true;
            return;
        }
        _ => {}
    }
    let OpenMenu::Machine(id) = *menu else {
        return;
    };
    let Some(building) = sim.world.building_mut(id) else {
        return;
    };
    message.0 = match (*action, &mut building.machine) {
        (Action::Toggle, machine) => {
            let on = !machine.enabled();
            machine.set_enabled(on);
            format!(
                "{} {}.",
                machine.kind().name(),
                if on { "allumé" } else { "éteint" }
            )
        }
        (Action::TakeAll, Machine::Extractor(e)) => {
            let output = e.output_mut();
            let kind = output.kind();
            let moved = transfer(output, &mut inventory.0);
            take_message(kind, moved, output.count())
        }
        (Action::TakeAll, Machine::Furnace(f)) => match f.output().kind() {
            None => "La sortie est vide.".to_string(),
            Some(kind) => {
                let moved = take_from_slot(f.output_mut(), &mut inventory.0);
                take_message(kind, moved, f.output().count())
            }
        },
        (Action::TakeAll, Machine::Storage(chest)) => {
            let moved = move_all(chest, &mut inventory.0);
            format!("{moved} objet(s) récupéré(s) du coffre.")
        }
        (Action::StoreAll, Machine::Storage(chest)) => {
            let moved = move_all(&mut inventory.0, chest);
            format!("{moved} objet(s) rangé(s) dans le coffre.")
        }
        (Action::Deposit, Machine::Furnace(f)) => match f.load_from(&mut inventory.0) {
            Some((kind, moved)) => format!(
                "{} {} dans le four.",
                kind.quantity(moved),
                kind.agree(moved, "déposé")
            ),
            None if f.input().count() >= f.input().capacity() => "L'entrée est pleine.".to_string(),
            None => "Aucun minerai compatible dans l'inventaire.".to_string(),
        },
        _ => return,
    };
    info!(
        "{} #{} : {}",
        building.machine.kind().name(),
        id.0,
        message.0
    );
}

/// Déplace tout ce qui rentre d'un inventaire vers un autre ; retourne le nombre déplacé.
fn move_all(from: &mut Inventory, to: &mut Inventory) -> u32 {
    let mut moved = 0;
    for index in 0..from.slots().len() {
        let Some(stack) = from.slot(index) else {
            continue;
        };
        let wanted = stack.count.min(to.space_for(stack.kind));
        let taken = from.take_at(index, wanted);
        let added = to.add(stack.kind, taken);
        debug_assert_eq!(taken, added, "conservation des objets");
        moved += added;
    }
    moved
}

fn take_message(kind: ItemKind, moved: u32, left: u32) -> String {
    let taken = |n: u32| kind.agree(n, "récupéré");
    match (moved, left) {
        (0, 0) => "La sortie est vide.".to_string(),
        (0, _) => format!(
            "Inventaire plein : impossible de prendre {}.",
            kind.quantity(left)
        ),
        (_, 0) => format!("{} {}.", kind.quantity(moved), taken(moved)),
        _ => format!(
            "{} {} ; inventaire plein, {left} {} dans la machine.",
            kind.quantity(moved),
            taken(moved),
            if left > 1 { "restent" } else { "reste" }
        ),
    }
}

/// Début d'un glisser-déposer : on retient la case d'origine (pour l'icône sous la souris).
fn on_drag_start(
    drag: On<PointerDragStart>,
    slots: Query<&SlotBox>,
    mut dragging: ResMut<Dragging>,
) {
    if drag.button == PointerButton::Primary
        && let Ok(SlotBox(slot)) = slots.get(drag.entity)
    {
        dragging.0 = Some(*slot);
    }
}

fn on_drag_end(_drag: On<PointerDragEnd>, mut dragging: ResMut<Dragging>) {
    dragging.0 = None;
}

/// Lâcher d'une case sur une autre : applique le déplacement (avec conservation).
fn on_drop(
    drop: On<PointerDragDrop>,
    slots: Query<&SlotBox>,
    menu: Res<OpenMenu>,
    mut sim: ResMut<Simulation>,
    mut inventory: ResMut<PlayerInventory>,
    mut message: ResMut<PanelMessage>,
) {
    if drop.button != PointerButton::Primary {
        return;
    }
    let (Ok(SlotBox(from)), Ok(SlotBox(to))) = (slots.get(drop.dropped), slots.get(drop.entity))
    else {
        return;
    };
    let machine = match *menu {
        OpenMenu::Machine(id) => sim.world.building_mut(id).map(|b| &mut b.machine),
        _ => None,
    };
    if let Some(text) = apply_drag(*from, *to, machine, &mut inventory.0) {
        message.0 = text;
    }
}

/// Applique un glisser-déposer entre deux cases. Retourne un message en cas de refus.
fn apply_drag(
    from: SlotRef,
    to: SlotRef,
    machine: Option<&mut Machine>,
    inventory: &mut Inventory,
) -> Option<String> {
    use SlotRef::{Input, Output, Storage};
    match (from, to, machine) {
        (SlotRef::Inventory(i), SlotRef::Inventory(j), _) => inventory.move_within(i, j),
        (SlotRef::Inventory(i), Storage(j), Some(Machine::Storage(chest))) => {
            move_between(inventory, i, chest, j);
        }
        (Storage(i), SlotRef::Inventory(j), Some(Machine::Storage(chest))) => {
            move_between(chest, i, inventory, j);
        }
        (Storage(i), Storage(j), Some(Machine::Storage(chest))) => chest.move_within(i, j),
        (SlotRef::Inventory(i), Input, Some(Machine::Furnace(f))) => {
            let stack = inventory.slot(i)?;
            let wanted = stack.count.min(f.input_space_for(stack.kind));
            if wanted == 0 {
                return Some(format!(
                    "Le four n'accepte pas de {} ici.",
                    stack.kind.name()
                ));
            }
            let taken = inventory.take_at(i, wanted);
            let added = f.insert(stack.kind, taken);
            debug_assert_eq!(taken, added, "conservation des objets");
        }
        (Input | Output, SlotRef::Inventory(j), Some(machine)) => {
            // Reprendre à la main l'entrée ou la sortie d'une machine, vers une case précise.
            let (kind, count) = match (from, &*machine) {
                (Output, Machine::Extractor(e)) => (e.output().kind(), e.output().count()),
                (Output, Machine::Furnace(f)) => f.output().stack().map(|s| (s.kind, s.count))?,
                (Input, Machine::Furnace(f)) => f.input().stack().map(|s| (s.kind, s.count))?,
                _ => return None,
            };
            let wanted = count.min(inventory.space_at(j, kind));
            if wanted == 0 {
                return (count > 0).then(|| "Cette case contient déjà autre chose.".to_string());
            }
            let taken = match (from, machine) {
                (Output, Machine::Extractor(e)) => e.output_mut().take(wanted),
                (Output, Machine::Furnace(f)) => f.output_mut().take(wanted),
                (Input, Machine::Furnace(f)) => f.input_mut().take(wanted),
                _ => 0,
            };
            let added = inventory.add_at(j, kind, taken);
            debug_assert_eq!(taken, added, "conservation des objets");
        }
        _ => return Some("Impossible de déposer ici.".to_string()),
    }
    None
}

fn item_color(kind: ItemKind) -> Color {
    match kind {
        ItemKind::IronOre => Color::srgb(0.50, 0.26, 0.18),
        ItemKind::CopperOre => Color::srgb(0.78, 0.46, 0.20),
        ItemKind::IronPlate => Color::srgb(0.55, 0.58, 0.62),
        ItemKind::CopperPlate => Color::srgb(0.85, 0.52, 0.32),
    }
}

/// Ligne d'état électrique d'un bâtiment.
fn power_line(world: &SimWorld, id: BuildingId, consumer: bool) -> String {
    let Some(stats) = world.network_stats(id) else {
        return String::new();
    };
    if stats.members.len() == 1 {
        return if consumer {
            format!(
                "Non relié : pose un poteau à moins de {POLE_RANGE_M:.0} m, relié à un générateur"
            )
        } else {
            format!("Isolé : pose un poteau à moins de {POLE_RANGE_M:.0} m")
        };
    }
    format!(
        "Réseau de {} bâtiments : {} kW produits · {} kW demandés · alimentation {} %",
        stats.members.len(),
        stats.produced_w / 1000,
        stats.demanded_w / 1000,
        stats.satisfaction / 10
    )
}

/// Objets à livrer d'un palier : « 23/50 plaques de cuivre · 120/300 plaques de fer ».
fn tier_progress(world: &SimWorld, tier: &Tier) -> String {
    let deliveries = world.deliveries();
    tier.requires
        .iter()
        .map(|&(kind, amount)| {
            let plural = kind.quantity(2);
            let unit = plural.split_once(' ').map_or("", |(_, unit)| unit);
            format!("{}/{amount} {unit}", deliveries.total(kind).min(amount))
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Ce que débloque un palier : « Convoyeur », ou « rien de plus pour l'instant ».
fn tier_unlocks(tier: &Tier) -> String {
    if tier.unlocks.is_empty() {
        "rien de plus pour l'instant".into()
    } else {
        tier.unlocks
            .iter()
            .map(|u| u.name())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Ligne de progression : prochain palier, livraisons, ce qu'il débloque.
fn goal_line(world: &SimWorld, creative: bool) -> String {
    if creative {
        return "Mode créatif : tout est débloqué, construction gratuite".into();
    }
    let reached = tier_reached(world.deliveries());
    let last = TIERS.len() - 1;
    match TIERS.get(reached + 1) {
        Some(tier) => format!(
            "Palier {}/{last} « {} » : livrer au hub {}\n→ débloque : {}",
            reached + 1,
            tier.name,
            tier_progress(world, tier),
            tier_unlocks(tier)
        ),
        None => format!("Tous les paliers sont débloqués ({last}/{last}) !"),
    }
}

/// Palier déjà annoncé : pas de notification pour ce qui était débloqué au chargement.
#[derive(Resource, Debug, Default)]
struct AnnouncedTier(usize);

fn init_announced_tier(mut commands: Commands, sim: Res<Simulation>) {
    commands.insert_resource(AnnouncedTier(tier_reached(sim.world.deliveries())));
}

/// Progression des paliers en haut à droite et notification temporaire au centre.
fn spawn_goal_hud(mut commands: Commands) {
    commands.spawn((
        GoalTracker,
        text("", 15.0, Color::WHITE),
        text_shadow(),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            right: Val::Px(12.0),
            ..default()
        },
        Pickable::IGNORE,
    ));
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                top: Val::Px(90.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_child((
            Toast,
            text("", 22.0, ACCENT),
            text_shadow(),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            Node {
                padding: UiRect::axes(Val::Px(18.0), Val::Px(10.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
        ));
}

fn update_goal_hud(
    sim: Res<Simulation>,
    session: Res<GameSession>,
    time: Res<Time>,
    mut announced: ResMut<AnnouncedTier>,
    mut until: ResMut<ToastUntil>,
    mut tracker: Single<&mut Text, (With<GoalTracker>, Without<Toast>)>,
    toast: Single<(&mut Text, &mut Visibility), (With<Toast>, Without<GoalTracker>)>,
) {
    let wanted = goal_line(&sim.world, session.creative);
    if tracker.0 != wanted {
        tracker.0 = wanted;
    }

    let (mut toast_text, mut visibility) = toast.into_inner();
    let reached = tier_reached(sim.world.deliveries());
    if reached > announced.0 {
        let tier = &TIERS[reached];
        toast_text.0 = if session.creative {
            format!("Palier « {} » atteint !", tier.name)
        } else {
            format!(
                "Palier « {} » débloqué : {} !",
                tier.name,
                tier_unlocks(tier)
            )
        };
        info!("{}", toast_text.0);
        until.0 = time.elapsed_secs() + 6.0;
        announced.0 = reached;
    }
    let show = time.elapsed_secs() < until.0;
    let wanted = if show {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if *visibility != wanted {
        *visibility = wanted;
    }
}

/// Ligne « Modules » : liste et statistiques résultantes de la machine.
fn modules_line(world: &SimWorld, id: BuildingId, machine: &Machine) -> String {
    let slots = machine.kind().module_slots();
    let modules = world.modules_of(id);
    let names: Vec<&str> = modules.iter().map(|(_, m)| m.kind.short_name()).collect();
    let stats = world.stats_of(id);
    let demand = match machine {
        Machine::Extractor(e) => e.power_demand_w(),
        Machine::Furnace(f) => f.power_demand_w(),
        _ => 0,
    };
    if stats == MachineStats::BASE && modules.is_empty() {
        return format!("Modules (0/{slots}) : aucun — fixe-en depuis le menu de construction (B)");
    }
    format!(
        "Modules ({}/{slots}) : {} · vitesse {} % · consommation {} % ({} kW en marche)",
        modules.len(),
        names.join(", "),
        stats.speed_percent,
        stats.power_percent,
        demand / 1000
    )
}

/// Ligne décrivant les convoyeurs branchés sur un bâtiment.
fn belts_line(world: &SimWorld, id: BuildingId) -> String {
    let incoming = world.conveyors().filter(|(_, c)| c.to == id).count();
    let outgoing = world.conveyors().filter(|(_, c)| c.from == id).count();
    match (incoming, outgoing) {
        (0, 0) => String::new(),
        _ => format!(
            "\nConvoyeurs : {} en entrée · {} en sortie ({}/min max chacun)",
            incoming,
            outgoing,
            conveyor_max_rate_per_min()
        ),
    }
}

fn machine_info(world: &SimWorld, deposits: &Deposits, id: BuildingId) -> String {
    let Some(building) = world.building(id) else {
        return String::new();
    };
    let now = world.clock().tick();
    let belts = belts_line(world, id);
    match &building.machine {
        Machine::Extractor(e) => {
            let deposit = deposits.0.get(e.deposit);
            format!(
                "Extracteur #{} — gisement de {} {}\n\
                 État : {} · {:.1}/min théorique (nominal {}) · {:.1}/min réel · {} produits\n{}{belts}",
                id.0,
                deposit.map_or("?", |d| d.kind.name()),
                deposit.map_or("?", |d| d.purity.name()),
                e.state().name(),
                e.effective_rate_per_min(),
                e.rate_per_min(),
                e.measured_rate_per_min(now),
                e.produced_total(),
                power_line(world, id, true),
            )
        }
        Machine::Furnace(f) => {
            let recipe = f.recipe().map_or_else(
                || "aucune recette : dépose du minerai".to_string(),
                |r| {
                    // Énergie par plaque : puissance demandée × durée réelle (vitesse comprise).
                    let energy_kj = r.energy_j(f.power_demand_w()) * 100
                        / u64::from(f.stats.speed_percent)
                        / 1000;
                    format!(
                        "{} → {} ({:.2} s, {energy_kj} kJ)",
                        r.input.name(),
                        r.output.name(),
                        f.craft_seconds(r)
                    )
                },
            );
            format!(
                "Four #{} — {recipe}\nÉtat : {} · {} fabrication(s) terminée(s)\n{}{belts}",
                id.0,
                f.state().name(),
                f.crafted_total(),
                power_line(world, id, true),
            )
        }
        Machine::Storage(chest) => {
            let used = chest.slots().iter().flatten().count();
            let total: u32 = ItemKind::ALL.iter().map(|k| chest.count(*k)).sum();
            format!(
                "Coffre #{} — {total} objet(s) · {used}/{STORAGE_SLOTS} cases occupées\n\
                 Entrée à l'arrière (bloc bleu), sortie à l'avant (bloc jaune){belts}",
                id.0
            )
        }
        Machine::Hub => {
            let deliveries = world.deliveries();
            let totals: Vec<String> = ItemKind::ALL
                .into_iter()
                .filter(|k| deliveries.total(*k) > 0)
                .map(|k| k.quantity(deliveries.total(k) as u32))
                .collect();
            format!(
                "Hub de livraison #{} — tous hubs confondus : {} objet(s) livré(s), {}/min\n{}\nTotaux : {}{belts}",
                id.0,
                deliveries.grand_total(),
                deliveries.per_minute(now),
                goal_line(world, false),
                if totals.is_empty() {
                    "rien pour l'instant".to_string()
                } else {
                    totals.join(" · ")
                }
            )
        }
        Machine::Generator(g) => format!(
            "Générateur #{} — {}\n{}",
            id.0,
            if g.enabled {
                format!("produit {} kW", GENERATOR_POWER_W / 1000)
            } else {
                "éteint".to_string()
            },
            power_line(world, id, false),
        ),
        Machine::Pole => format!(
            "Poteau électrique #{} — portée {POLE_RANGE_M:.0} m\n{}",
            id.0,
            power_line(world, id, false),
        ),
    }
}

/// Contenu d'une case : (objet, quantité), ou `None` si elle est vide.
fn slot_content(
    slot: SlotRef,
    machine: Option<&Machine>,
    inventory: &Inventory,
) -> Option<(ItemKind, u32)> {
    let stack = match (slot, machine) {
        (SlotRef::Inventory(i), _) => inventory.slot(i).map(|s| (s.kind, s.count)),
        (SlotRef::Storage(i), Some(Machine::Storage(chest))) => {
            chest.slot(i).map(|s| (s.kind, s.count))
        }
        (SlotRef::Output, Some(Machine::Extractor(e))) => {
            Some((e.output().kind(), e.output().count()))
        }
        (SlotRef::Output, Some(Machine::Furnace(f))) => {
            f.output().stack().map(|s| (s.kind, s.count))
        }
        (SlotRef::Input, Some(Machine::Furnace(f))) => f.input().stack().map(|s| (s.kind, s.count)),
        _ => None,
    };
    stack.filter(|(_, count)| *count > 0)
}

fn update_menu(
    menu: Res<OpenMenu>,
    sim: Res<Simulation>,
    deposits: Res<Deposits>,
    inventory: Res<PlayerInventory>,
    message: Res<PanelMessage>,
    dragging: Res<Dragging>,
    mut boxes: Query<(&SlotBox, &mut BackgroundColor, &mut BorderColor)>,
    mut texts: Query<(
        &mut Text,
        Option<&SlotText>,
        Has<MachineInfo>,
        Has<MessageText>,
        Has<InventoryTitle>,
        Has<ToggleLabel>,
        Has<ModulesInfo>,
    )>,
    mut fill: Query<&mut Node, With<ProgressFill>>,
    mut next: ResMut<NextState<PlayState>>,
) {
    let machine = match *menu {
        OpenMenu::Machine(id) => match sim.world.building(id) {
            Some(b) => Some((id, &b.machine)),
            None => {
                // La machine a disparu : on referme.
                next.set(PlayState::Running);
                return;
            }
        },
        _ => None,
    };
    let content = |slot| slot_content(slot, machine.map(|(_, m)| m), &inventory.0);

    for (SlotBox(slot), mut background, mut border) in &mut boxes {
        let color = content(*slot).map_or(SLOT_EMPTY, |(kind, _)| item_color(kind));
        if background.0 != color {
            background.0 = color;
        }
        // La case d'origine du glisser-déposer est entourée.
        let edge = if dragging.0 == Some(*slot) {
            ACCENT
        } else {
            SLOT_BORDER
        };
        if border.top != edge {
            border.set_all(edge);
        }
    }

    for (mut text, slot, is_info, is_message, is_title, is_toggle, is_modules) in &mut texts {
        let wanted = if let Some(SlotText(slot)) = slot {
            content(*slot).map_or_else(String::new, |(kind, count)| {
                format!("{}\n{count}", kind.symbol())
            })
        } else if is_info {
            machine.map_or_else(String::new, |(id, _)| {
                machine_info(&sim.world, &deposits, id)
            })
        } else if is_message {
            message.0.clone()
        } else if is_title {
            let totals: Vec<String> = ItemKind::ALL
                .into_iter()
                .map(|kind| (kind, inventory.0.count(kind)))
                .filter(|(_, count)| *count > 0)
                .map(|(kind, count)| kind.quantity(count))
                .collect();
            if totals.is_empty() {
                "Inventaire — vide".to_string()
            } else {
                format!("Inventaire — {}", totals.join(" · "))
            }
        } else if is_toggle {
            let on = machine.is_some_and(|(_, m)| m.enabled());
            (if on { "Éteindre" } else { "Allumer" }).to_string()
        } else if is_modules {
            machine.map_or_else(String::new, |(id, m)| modules_line(&sim.world, id, m))
        } else {
            continue;
        };
        if text.0 != wanted {
            text.0 = wanted;
        }
    }

    let progress = match machine.map(|(_, m)| m) {
        Some(Machine::Extractor(e)) => Some(e.progress_fraction()),
        Some(Machine::Furnace(f)) => Some(f.progress_fraction()),
        _ => None,
    };
    if let (Some(progress), Ok(mut node)) = (progress, fill.single_mut()) {
        node.width = Val::Percent(progress * 100.0);
    }
}

/// Fait suivre la souris à l'icône du glisser-déposer.
fn follow_cursor(
    window: Single<&Window, With<PrimaryWindow>>,
    dragging: Res<Dragging>,
    menu: Res<OpenMenu>,
    sim: Res<Simulation>,
    inventory: Res<PlayerInventory>,
    ghost: Single<(&mut Node, &mut Visibility, &mut BackgroundColor), With<DragGhost>>,
    mut label: Single<&mut Text, With<DragGhostText>>,
) {
    let (mut node, mut visibility, mut background) = ghost.into_inner();
    let machine = match *menu {
        OpenMenu::Machine(id) => sim.world.building(id).map(|b| &b.machine),
        _ => None,
    };
    let content = dragging
        .0
        .and_then(|slot| slot_content(slot, machine, &inventory.0));
    match (content, window.cursor_position()) {
        (Some((kind, count)), Some(cursor)) => {
            *visibility = Visibility::Inherited;
            node.left = Val::Px(cursor.x - SLOT_PX / 2.0);
            node.top = Val::Px(cursor.y - SLOT_PX / 2.0);
            background.0 = item_color(kind).with_alpha(0.85);
            let wanted = format!("{}\n{count}", kind.symbol());
            if label.0 != wanted {
                label.0 = wanted;
            }
        }
        _ => {
            if *visibility != Visibility::Hidden {
                *visibility = Visibility::Hidden;
            }
        }
    }
}

fn style_buttons(
    mut buttons: Query<
        (
            &Hovered,
            &mut BackgroundColor,
            Has<Card>,
            Has<screens::Danger>,
        ),
        Changed<Hovered>,
    >,
) {
    for (hovered, mut background, is_card, danger) in &mut buttons {
        background.0 = match (hovered.get(), is_card, danger) {
            (true, _, true) => screens::DANGER_HOVER,
            (false, _, true) => screens::DANGER_IDLE,
            (true, true, _) => CARD_HOVER,
            (false, true, _) => CARD_IDLE,
            (true, false, _) => BUTTON_HOVER,
            (false, false, _) => BUTTON_IDLE,
        };
    }
}

fn spawn_prompt(mut commands: Commands) {
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                // Décalé sous le réticule.
                padding: UiRect::top(Val::Px(80.0)),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_child((
            Prompt,
            text("", 15.0, Color::WHITE),
            text_shadow(),
            Pickable::IGNORE,
        ));
}

/// Aide contextuelle sous le réticule selon ce qui est visé.
fn update_prompt(
    state: Res<State<PlayState>>,
    target: Res<Target>,
    build: Res<BuildMode>,
    sim: Res<Simulation>,
    mut prompt: Single<&mut Text, With<Prompt>>,
) {
    let wanted = match (state.get(), target.0) {
        (PlayState::Running, Aim::Building { id, .. }) => {
            match sim.world.building(id).map(|b| b.machine.kind()) {
                Some(kind) if build.active => {
                    format!("E : ouvrir ({}) · Clic droit : démonter", kind.name())
                }
                Some(kind) => format!("E : ouvrir ({})", kind.name()),
                None => String::new(),
            }
        }
        (PlayState::Running, Aim::Module { id, .. }) => match sim.world.module(id) {
            Some(m) => format!(
                "{} sur #{} · E : ouvrir la machine{}",
                m.kind.name(),
                m.host.0,
                if build.active {
                    " · Clic droit : retirer"
                } else {
                    ""
                }
            ),
            None => String::new(),
        },
        (PlayState::Running, Aim::Conveyor { id, .. }) => match sim.world.conveyor(id) {
            Some(c) => format!(
                "Convoyeur #{} · {} objet(s) en route · {} livrés{}",
                id.0,
                c.len(),
                c.delivered_total(),
                if build.active {
                    " · Clic droit : démonter"
                } else {
                    ""
                }
            ),
            None => String::new(),
        },
        (PlayState::Running, Aim::Rock { .. }) if !build.active => {
            "Clic gauche : détruire le rocher".to_string()
        }
        _ => String::new(),
    };
    if prompt.0 != wanted {
        prompt.0 = wanted;
    }
}

const HELP_TEXT: &str = "\
CONTRÔLES
Z Q S D : se déplacer · Souris : regarder · Maj : courir
Espace : sauter (marche) / monter (vol) · Ctrl : descendre (vol)
V : marche / vol · Échap : pause (ou arrêter de construire) · Maj + Échap : quitter

CONSTRUIRE
B : menu de construction · Clic gauche : poser · Clic droit : démonter
R : tourner · Convoyeur : sortie jaune, puis entrée bleue (flèches = sens)
Modules : vise une machine, ils se posent sur le toit
Chaque pose coûte des plaques (remboursées au démontage) ; mode créatif : gratuit

PROGRESSION
Livre des plaques au hub pour débloquer les paliers (en haut à droite)
Logistique : convoyeurs · Accélération : module de vitesse · Rendement : module d'efficacité

PARTIE
F5 : sauvegarder · sauvegarde automatique toutes les 2 min et en quittant
Échap : pause, puis « revenir au menu » pour changer de monde

INTERAGIR
E : ouvrir la machine visée · Tab : inventaire (glisser-déposer)
Clic gauche hors construction : détruire un rocher

VOYANTS
Vert : actif · Orange : en attente ou sortie pleine
Rouge clignotant : sans énergie · Gris : éteint
Fantôme vert : pose possible · rouge : impossible (motif en bas)

COULEURS
Orange : extraction · Violet : énergie · Brique : fours
Bois : stockage · Blanc et or : hub de livraison

OUTILS DE TEST
T : vitesse du temps ×1 / ×10 / ×100 · G : +50 minerais de fer et de cuivre
I / O : remplir / vider la machine visée · F3 : overlay debug · F12 : capture

H : fermer l'aide";

/// Panneau d'aide (masqué) et rappel « H : aide » en bas à gauche.
fn spawn_help(mut commands: Commands) {
    commands.spawn((
        text("H : aide", 14.0, MUTED),
        text_shadow(),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(10.0),
            left: Val::Px(12.0),
            ..default()
        },
        Pickable::IGNORE,
    ));
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::Center,
                padding: UiRect::right(Val::Px(16.0)),
                ..default()
            },
            Pickable::IGNORE,
            GlobalZIndex(15),
        ))
        .with_child((
            HelpPanel,
            text(HELP_TEXT, 14.0, Color::WHITE),
            BackgroundColor(PANEL_BG),
            Node {
                padding: UiRect::all(Val::Px(16.0)),
                border_radius: BorderRadius::all(Val::Px(10.0)),
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
        ));
}

fn toggle_help(
    keys: Res<ButtonInput<KeyCode>>,
    mut help: Single<&mut Visibility, With<HelpPanel>>,
) {
    if keys.just_pressed(KeyCode::KeyH) {
        help.toggle_visible_hidden();
    }
}
