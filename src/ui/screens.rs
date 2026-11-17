//! Écran titre (mondes sauvegardés, nouveau monde), menu pause et bandeau de sauvegarde.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::picking::Pickable;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};

use super::{ACCENT, BUTTON_IDLE, CARD_IDLE, MUTED, PANEL_BG, text, text_shadow};
use crate::app::PlayState;
use crate::config::random_seed;
use crate::game::{GameSession, Screen, WorldSetup, start_loaded_game, start_new_game};
use crate::persistence::{
    MAX_NAME_CHARS, SlotInfo, clean_name, delete_slot, list_slots, new_slot_id, now_unix,
    read_slot, restore_backup, save_dir,
};
use crate::save::{AfterSave, SaveRequest, SaveStatus};
use crate::settings::{OptionsOpen, SettingKey, Settings};
use crate::simulation::clock::TICK_RATE_HZ;
use crate::world::generation::WorldSeed;

/// Fond des boutons « dangereux » (suppression).
pub(super) const DANGER_IDLE: Color = Color::srgb(0.50, 0.18, 0.16);
pub(super) const DANGER_HOVER: Color = Color::srgb(0.66, 0.24, 0.21);
const TITLE_BG: Color = Color::srgb(0.07, 0.09, 0.12);
const FIELD_FOCUS: Color = Color::srgb(0.24, 0.30, 0.38);
/// Nombre maximal de chiffres de la seed.
const MAX_SEED_DIGITS: usize = 18;

/// Bouton au fond rouge (le survol garde la teinte).
#[derive(Component)]
pub(super) struct Danger;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Name,
    Seed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TitleAction {
    Play(String),
    AskDelete(String),
    ConfirmDelete(String),
    CancelDelete,
    RestoreBackup(String),
    Focus(Field),
    ToggleCreative,
    Create,
    Options,
    Quit,
}

#[derive(Component)]
struct TitleButton(TitleAction);

#[derive(Component)]
struct TitleRoot;

#[derive(Component)]
struct FieldText(Field);

/// Saisie de l'écran titre.
#[derive(Resource, Debug, Default)]
struct TitleForm {
    name: String,
    seed: String,
    creative: bool,
    focus: Option<Field>,
    /// Emplacement dont la suppression attend confirmation.
    confirm_delete: Option<String>,
    message: String,
    /// L'écran doit être reconstruit (liste ou boutons changés).
    dirty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PauseAction {
    Resume,
    Save,
    Options,
    Title,
    Quit,
}

#[derive(Component)]
struct PauseButton(PauseAction);

#[derive(Component)]
struct PauseMenu;

#[derive(Component)]
struct SaveToast;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OptionAction {
    Step(SettingKey, i32),
    Defaults,
    Close,
}

#[derive(Component)]
struct OptionButton(OptionAction);

#[derive(Component)]
struct OptionsRoot;

pub struct ScreensPlugin;

impl Plugin for ScreensPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TitleForm>()
            .add_systems(OnEnter(Screen::Title), enter_title)
            .add_systems(
                Update,
                (type_in_field, rebuild_title, update_field_texts)
                    .chain()
                    .run_if(in_state(Screen::Title)),
            )
            .add_systems(
                OnEnter(Screen::InGame),
                (spawn_pause_menu, spawn_save_toast).in_set(WorldSetup::Spawn),
            )
            .add_systems(OnEnter(PlayState::Paused), show_pause_menu::<true>)
            .add_systems(OnExit(PlayState::Paused), show_pause_menu::<false>)
            .add_systems(Update, update_save_toast.run_if(in_state(Screen::InGame)))
            .add_systems(
                Update,
                (
                    close_options_on_escape.run_if(in_state(Screen::Title)),
                    sync_options_panel,
                )
                    .chain(),
            )
            .add_systems(OnExit(Screen::Title), close_options)
            .add_systems(OnExit(PlayState::Paused), close_options);
    }
}

// ---------------------------------------------------------------------------
// Écran titre
// ---------------------------------------------------------------------------

fn enter_title(mut commands: Commands, mut form: ResMut<TitleForm>) {
    commands.spawn((Camera2d, DespawnOnExit(Screen::Title)));
    let worlds = list_slots(&save_dir()).len();
    *form = TitleForm {
        name: format!("Monde {}", worlds + 1),
        focus: Some(Field::Name),
        dirty: true,
        ..default()
    };
}

/// Durée de jeu lisible : « 12 min », « 2 h 05 ».
fn play_time(ticks: u64) -> String {
    let minutes = ticks / u64::from(TICK_RATE_HZ) / 60;
    if minutes < 60 {
        format!("{minutes} min de jeu")
    } else {
        format!("{} h {:02} de jeu", minutes / 60, minutes % 60)
    }
}

/// « à l'instant », « il y a 5 min », « il y a 3 h », « il y a 2 j ».
fn ago(saved_unix: u64, now: u64) -> String {
    let secs = now.saturating_sub(saved_unix);
    match secs {
        0..60 => "à l'instant".into(),
        60..3600 => format!("il y a {} min", secs / 60),
        3600..86_400 => format!("il y a {} h", secs / 3600),
        _ => format!("il y a {} j", secs / 86_400),
    }
}

fn title_button(parent: &mut ChildSpawnerCommands, action: TitleAction, label: &str, danger: bool) {
    let mut button = parent.spawn((
        TitleButton(action),
        Button,
        Hovered::default(),
        Node {
            padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
            border_radius: BorderRadius::all(Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(if danger { DANGER_IDLE } else { BUTTON_IDLE }),
    ));
    if danger {
        button.insert(Danger);
    }
    button
        .observe(on_title_button)
        .with_child((text(label, 15.0, Color::WHITE), Pickable::IGNORE));
}

fn panel() -> (Node, BackgroundColor) {
    (
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(10.0),
            padding: UiRect::all(Val::Px(18.0)),
            border_radius: BorderRadius::all(Val::Px(10.0)),
            width: Val::Px(460.0),
            ..default()
        },
        BackgroundColor(PANEL_BG),
    )
}

fn rebuild_title(
    mut commands: Commands,
    mut form: ResMut<TitleForm>,
    roots: Query<Entity, With<TitleRoot>>,
) {
    if !form.dirty {
        return;
    }
    form.dirty = false;
    for root in &roots {
        commands.entity(root).despawn();
    }
    let slots = list_slots(&save_dir());
    let now = now_unix();

    commands
        .spawn((
            TitleRoot,
            DespawnOnExit(Screen::Title),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::top(Val::Px(48.0)),
                row_gap: Val::Px(20.0),
                ..default()
            },
            BackgroundColor(TITLE_BG),
        ))
        .with_children(|root| {
            root.spawn((text("PROJECT ASCENSION", 46.0, Color::WHITE), text_shadow()));
            root.spawn(text("Sandbox industriel · V0.1", 16.0, MUTED));
            root.spawn(Node {
                column_gap: Val::Px(24.0),
                align_items: AlignItems::FlexStart,
                ..default()
            })
            .with_children(|columns| {
                columns.spawn(panel()).with_children(|list| {
                    list.spawn(text("Mondes sauvegardés", 20.0, ACCENT));
                    if slots.is_empty() {
                        list.spawn(text(
                            "Aucun monde pour l'instant : crée le premier →",
                            15.0,
                            MUTED,
                        ));
                    }
                    for slot in &slots {
                        slot_row(list, slot, &form, now);
                    }
                });
                columns.spawn(panel()).with_children(|new| {
                    new_world_panel(new, &form);
                });
            });
            if !form.message.is_empty() {
                root.spawn((
                    text(form.message.clone(), 16.0, Color::srgb(1.0, 0.6, 0.5)),
                    text_shadow(),
                ));
            }
            root.spawn(Node {
                column_gap: Val::Px(12.0),
                ..default()
            })
            .with_children(|row| {
                title_button(row, TitleAction::Options, "Options", false);
                title_button(row, TitleAction::Quit, "Quitter", false);
            });
            root.spawn(text(
                format!("Sauvegardes : {}", save_dir().display()),
                12.0,
                MUTED,
            ));
        });
}

fn slot_row(list: &mut ChildSpawnerCommands, slot: &SlotInfo, form: &TitleForm, now: u64) {
    list.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(6.0),
            padding: UiRect::all(Val::Px(10.0)),
            border_radius: BorderRadius::all(Val::Px(8.0)),
            ..default()
        },
        BackgroundColor(CARD_IDLE),
    ))
    .with_children(|card| {
        match &slot.meta {
            Ok(meta) => {
                card.spawn(text(meta.name.clone(), 18.0, Color::WHITE));
                let mut details = format!(
                    "seed {} · {} · {}",
                    meta.seed.0,
                    play_time(meta.ticks),
                    ago(meta.saved_unix, now)
                );
                if meta.creative {
                    details.push_str(" · créatif");
                }
                card.spawn(text(details, 13.0, MUTED));
            }
            Err(error) => {
                card.spawn(text(slot.id.clone(), 18.0, Color::WHITE));
                card.spawn(text(error.message(), 13.0, Color::srgb(1.0, 0.6, 0.5)));
            }
        }
        card.spawn(Node {
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|buttons| {
            if form.confirm_delete.as_deref() == Some(slot.id.as_str()) {
                buttons.spawn(text("Supprimer définitivement ?", 14.0, Color::WHITE));
                title_button(
                    buttons,
                    TitleAction::ConfirmDelete(slot.id.clone()),
                    "Oui, supprimer",
                    true,
                );
                title_button(buttons, TitleAction::CancelDelete, "Annuler", false);
                return;
            }
            if slot.meta.is_ok() {
                title_button(buttons, TitleAction::Play(slot.id.clone()), "Jouer", false);
            } else if slot.has_backup {
                title_button(
                    buttons,
                    TitleAction::RestoreBackup(slot.id.clone()),
                    "Restaurer la copie précédente",
                    false,
                );
            }
            title_button(
                buttons,
                TitleAction::AskDelete(slot.id.clone()),
                "Supprimer",
                true,
            );
        });
    });
}

fn field(parent: &mut ChildSpawnerCommands, which: Field, label: &str) {
    parent.spawn(text(label, 14.0, MUTED));
    parent
        .spawn((
            TitleButton(TitleAction::Focus(which)),
            Button,
            Hovered::default(),
            Node {
                padding: UiRect::axes(Val::Px(10.0), Val::Px(8.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                min_height: Val::Px(36.0),
                ..default()
            },
            BackgroundColor(CARD_IDLE),
            super::Card,
        ))
        .observe(on_title_button)
        .with_child((
            FieldText(which),
            text("", 16.0, Color::WHITE),
            Pickable::IGNORE,
        ));
}

fn new_world_panel(panel: &mut ChildSpawnerCommands, form: &TitleForm) {
    panel.spawn(text("Nouveau monde", 20.0, ACCENT));
    field(panel, Field::Name, "Nom");
    field(panel, Field::Seed, "Seed (vide = aléatoire)");
    panel
        .spawn(Node {
            column_gap: Val::Px(10.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            let label = if form.creative {
                "Mode : créatif"
            } else {
                "Mode : normal"
            };
            title_button(row, TitleAction::ToggleCreative, label, false);
            let hint = if form.creative {
                "construction gratuite, tout débloqué"
            } else {
                "bâtiments payés en plaques, paliers à débloquer"
            };
            row.spawn(text(hint, 13.0, MUTED));
        });
    panel.spawn(Node::default()).with_children(|row| {
        title_button(row, TitleAction::Create, "Créer et jouer", false);
    });
    panel.spawn(text("Tab : changer de champ · Entrée : créer", 12.0, MUTED));
}

fn update_field_texts(form: Res<TitleForm>, mut texts: Query<(&FieldText, &mut Text)>) {
    for (FieldText(which), mut text) in &mut texts {
        let value = match which {
            Field::Name => &form.name,
            Field::Seed => &form.seed,
        };
        let caret = if form.focus == Some(*which) { "|" } else { "" };
        let wanted = format!("{value}{caret}");
        if text.0 != wanted {
            text.0 = wanted;
        }
    }
}

fn type_in_field(
    mut keys: MessageReader<KeyboardInput>,
    mut form: ResMut<TitleForm>,
    mut commands: Commands,
    mut next: ResMut<NextState<Screen>>,
    mut fields: Query<(&TitleButton, &mut BackgroundColor)>,
) {
    for key in keys.read() {
        if key.state != ButtonState::Pressed {
            continue;
        }
        match &key.logical_key {
            Key::Tab => {
                form.focus = Some(match form.focus {
                    Some(Field::Name) => Field::Seed,
                    _ => Field::Name,
                });
            }
            Key::Enter => create_world(&mut form, &mut commands, &mut next),
            Key::Backspace => match form.focus {
                Some(Field::Name) => {
                    form.name.pop();
                }
                Some(Field::Seed) => {
                    form.seed.pop();
                }
                None => {}
            },
            Key::Space if form.focus == Some(Field::Name) => push_name(&mut form, " "),
            Key::Character(typed) => match form.focus {
                Some(Field::Name) => push_name(&mut form, typed),
                Some(Field::Seed) => {
                    for c in typed.chars().filter(char::is_ascii_digit) {
                        if form.seed.len() < MAX_SEED_DIGITS {
                            form.seed.push(c);
                        }
                    }
                }
                None => {}
            },
            _ => {}
        }
    }
    // Champ actif en surbrillance.
    for (TitleButton(action), mut background) in &mut fields {
        if let TitleAction::Focus(which) = action {
            let wanted = if form.focus == Some(*which) {
                FIELD_FOCUS
            } else {
                CARD_IDLE
            };
            if background.0 != wanted {
                background.0 = wanted;
            }
        }
    }
}

fn push_name(form: &mut TitleForm, typed: &str) {
    for c in typed.chars().filter(|c| !c.is_control()) {
        if form.name.chars().count() < MAX_NAME_CHARS {
            form.name.push(c);
        }
    }
}

fn create_world(form: &mut TitleForm, commands: &mut Commands, next: &mut NextState<Screen>) {
    let Some(name) = clean_name(&form.name) else {
        form.message = format!("Donne un nom au monde ({MAX_NAME_CHARS} caractères au plus).");
        form.dirty = true;
        return;
    };
    let seed = match form.seed.parse::<u64>() {
        Ok(seed) => WorldSeed(seed),
        Err(_) => random_seed(),
    };
    let slot = new_slot_id(&save_dir(), &name);
    info!(
        "Nouveau monde « {name} » (seed {}, emplacement {slot})",
        seed.0
    );
    start_new_game(commands, Some(slot), name, seed, form.creative);
    next.set(Screen::InGame);
}

fn on_title_button(
    activate: On<Activate>,
    buttons: Query<&TitleButton>,
    mut form: ResMut<TitleForm>,
    mut commands: Commands,
    mut next: ResMut<NextState<Screen>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(TitleButton(action)) = buttons.get(activate.entity) else {
        return;
    };
    let dir = save_dir();
    match action.clone() {
        TitleAction::Play(id) => match read_slot(&dir, &id) {
            Ok(save) => {
                start_loaded_game(&mut commands, id, save);
                next.set(Screen::InGame);
            }
            Err(error) => {
                form.message = format!("Impossible de charger : {}", error.message());
                form.dirty = true;
            }
        },
        TitleAction::RestoreBackup(id) => match restore_backup(&dir, &id) {
            Ok(save) => {
                info!("Copie de secours restaurée pour {id}");
                start_loaded_game(&mut commands, id, save);
                next.set(Screen::InGame);
            }
            Err(error) => {
                form.message = format!("Copie de secours inutilisable : {}", error.message());
                form.dirty = true;
            }
        },
        TitleAction::AskDelete(id) => {
            form.confirm_delete = Some(id);
            form.dirty = true;
        }
        TitleAction::ConfirmDelete(id) => {
            form.message = match delete_slot(&dir, &id) {
                Ok(()) => String::new(),
                Err(error) => format!("Suppression impossible : {}", error.message()),
            };
            form.confirm_delete = None;
            form.dirty = true;
        }
        TitleAction::CancelDelete => {
            form.confirm_delete = None;
            form.dirty = true;
        }
        TitleAction::Focus(which) => form.focus = Some(which),
        TitleAction::ToggleCreative => {
            form.creative = !form.creative;
            form.dirty = true;
        }
        TitleAction::Create => create_world(&mut form, &mut commands, &mut next),
        TitleAction::Options => commands.insert_resource(OptionsOpen(true)),
        TitleAction::Quit => {
            exit.write(AppExit::Success);
        }
    }
}

// ---------------------------------------------------------------------------
// Menu pause
// ---------------------------------------------------------------------------

fn pause_button(parent: &mut ChildSpawnerCommands, action: PauseAction, label: &str) {
    parent
        .spawn((
            PauseButton(action),
            Button,
            Hovered::default(),
            Node {
                padding: UiRect::axes(Val::Px(18.0), Val::Px(10.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                justify_content: JustifyContent::Center,
                width: Val::Px(320.0),
                ..default()
            },
            BackgroundColor(BUTTON_IDLE),
        ))
        .observe(on_pause_button)
        .with_child((text(label, 17.0, Color::WHITE), Pickable::IGNORE));
}

fn spawn_pause_menu(mut commands: Commands, session: Res<GameSession>) {
    let saved = session.slot.is_some();
    commands
        .spawn((
            PauseMenu,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(10.0),
                    padding: UiRect::all(Val::Px(24.0)),
                    border_radius: BorderRadius::all(Val::Px(10.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
            ))
            .with_children(|menu| {
                menu.spawn(text("PAUSE", 30.0, Color::WHITE));
                let subtitle = if saved {
                    format!("Monde « {} »", session.name)
                } else {
                    format!("Monde « {} » · pas encore sauvegardé (F5)", session.name)
                };
                menu.spawn(text(subtitle, 15.0, MUTED));
                pause_button(menu, PauseAction::Resume, "Reprendre (clic ou Échap)");
                pause_button(menu, PauseAction::Save, "Sauvegarder (F5)");
                pause_button(menu, PauseAction::Options, "Options");
                let (title, quit) = if saved {
                    ("Sauvegarder et revenir au menu", "Sauvegarder et quitter")
                } else {
                    (
                        "Menu principal sans sauvegarder",
                        "Quitter sans sauvegarder",
                    )
                };
                pause_button(menu, PauseAction::Title, title);
                pause_button(menu, PauseAction::Quit, quit);
                menu.spawn(text(
                    "Tab : inventaire · E : machine · B : construction · H : aide complète",
                    14.0,
                    MUTED,
                ));
            });
        });
}

fn show_pause_menu<const VISIBLE: bool>(mut menu: Query<&mut Visibility, With<PauseMenu>>) {
    for mut visibility in &mut menu {
        *visibility = if VISIBLE {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn on_pause_button(
    activate: On<Activate>,
    buttons: Query<&PauseButton>,
    session: Res<GameSession>,
    mut options: ResMut<OptionsOpen>,
    mut requests: MessageWriter<SaveRequest>,
    mut next_play: ResMut<NextState<PlayState>>,
    mut next_screen: ResMut<NextState<Screen>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(PauseButton(action)) = buttons.get(activate.entity) else {
        return;
    };
    let saved = session.slot.is_some();
    match action {
        PauseAction::Resume => next_play.set(PlayState::Running),
        PauseAction::Save => {
            requests.write(SaveRequest {
                then: AfterSave::Continue,
            });
        }
        PauseAction::Options => options.0 = true,
        PauseAction::Title if saved => {
            requests.write(SaveRequest {
                then: AfterSave::Title,
            });
        }
        PauseAction::Title => next_screen.set(Screen::Title),
        PauseAction::Quit if saved => {
            requests.write(SaveRequest {
                then: AfterSave::Quit,
            });
        }
        PauseAction::Quit => {
            exit.write(AppExit::Success);
        }
    }
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

fn close_options(mut options: ResMut<OptionsOpen>) {
    options.0 = false;
}

fn close_options_on_escape(keys: Res<ButtonInput<KeyCode>>, mut options: ResMut<OptionsOpen>) {
    if options.0 && keys.just_pressed(KeyCode::Escape) {
        options.0 = false;
    }
}

fn option_button(parent: &mut ChildSpawnerCommands, action: OptionAction, label: &str, width: f32) {
    parent
        .spawn((
            OptionButton(action),
            Button,
            Hovered::default(),
            Node {
                width: Val::Px(width),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(BUTTON_IDLE),
        ))
        .observe(on_option_button)
        .with_child((text(label, 15.0, Color::WHITE), Pickable::IGNORE));
}

/// Ouvre, ferme ou redessine le panneau quand il s'ouvre ou qu'une option change.
fn sync_options_panel(
    options: Res<OptionsOpen>,
    settings: Res<Settings>,
    roots: Query<Entity, With<OptionsRoot>>,
    play: Option<Res<State<PlayState>>>,
    mut pause_menu: Query<&mut Visibility, With<PauseMenu>>,
    mut commands: Commands,
) {
    if !options.is_changed() && !settings.is_changed() {
        return;
    }
    // Le menu pause s'efface derrière les options, et revient quand on les ferme.
    let paused = play.is_some_and(|state| *state.get() == PlayState::Paused);
    for mut visibility in &mut pause_menu {
        *visibility = if paused && !options.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    if !options.0 {
        return;
    }
    commands
        .spawn((
            OptionsRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            GlobalZIndex(30),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(8.0),
                    padding: UiRect::all(Val::Px(22.0)),
                    border_radius: BorderRadius::all(Val::Px(10.0)),
                    width: Val::Px(620.0),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
            ))
            .with_children(|panel| {
                panel.spawn(text("Options", 26.0, Color::WHITE));
                for (section, keys) in SettingKey::SECTIONS {
                    panel.spawn((
                        text(section, 17.0, ACCENT),
                        Node {
                            margin: UiRect::top(Val::Px(8.0)),
                            ..default()
                        },
                    ));
                    for &key in keys {
                        option_row(panel, key, &settings);
                    }
                }
                panel
                    .spawn(Node {
                        column_gap: Val::Px(10.0),
                        margin: UiRect::top(Val::Px(12.0)),
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        option_button(row, OptionAction::Close, "Retour (Échap)", 160.0);
                        option_button(row, OptionAction::Defaults, "Valeurs par défaut", 180.0);
                        row.spawn(text("Enregistrées automatiquement", 13.0, MUTED));
                    });
            });
        });
}

fn option_row(panel: &mut ChildSpawnerCommands, key: SettingKey, settings: &Settings) {
    panel
        .spawn(Node {
            align_items: AlignItems::Center,
            column_gap: Val::Px(8.0),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                text(key.label(), 15.0, Color::WHITE),
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
            ));
            let value = key.value(settings);
            if key.is_toggle() {
                // Même largeur que « ‹ valeur › » : 36 + 8 + 132 + 8 + 36.
                option_button(row, OptionAction::Step(key, 1), &value, 220.0);
                return;
            }
            option_button(row, OptionAction::Step(key, -1), "‹", 36.0);
            row.spawn((
                text(value, 15.0, ACCENT),
                Node {
                    width: Val::Px(132.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                TextLayout::justify(Justify::Center),
            ));
            option_button(row, OptionAction::Step(key, 1), "›", 36.0);
        });
}

fn on_option_button(
    activate: On<Activate>,
    buttons: Query<&OptionButton>,
    mut settings: ResMut<Settings>,
    mut options: ResMut<OptionsOpen>,
) {
    let Ok(OptionButton(action)) = buttons.get(activate.entity) else {
        return;
    };
    match *action {
        OptionAction::Step(key, direction) => key.step(&mut settings, direction),
        OptionAction::Defaults => *settings = Settings::default(),
        OptionAction::Close => options.0 = false,
    }
}

// ---------------------------------------------------------------------------
// Bandeau « Partie sauvegardée »
// ---------------------------------------------------------------------------

fn spawn_save_toast(mut commands: Commands) {
    commands.spawn((
        SaveToast,
        text("", 15.0, ACCENT),
        text_shadow(),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(12.0),
            right: Val::Px(14.0),
            ..default()
        },
        Visibility::Hidden,
        Pickable::IGNORE,
    ));
}

fn update_save_toast(
    status: Res<SaveStatus>,
    time: Res<Time<Real>>,
    toast: Single<(&mut Text, &mut TextColor, &mut Visibility), With<SaveToast>>,
) {
    let (mut text, mut color, mut visibility) = toast.into_inner();
    let show = time.elapsed_secs() < status.until;
    let wanted = if show {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if *visibility != wanted {
        *visibility = wanted;
    }
    if show && text.0 != status.message {
        text.0 = status.message.clone();
        color.0 = if status.failed {
            Color::srgb(1.0, 0.55, 0.45)
        } else {
            ACCENT
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_naturally() {
        assert_eq!(play_time(0), "0 min de jeu");
        assert_eq!(play_time(20 * 60 * 75), "1 h 15 de jeu");
        assert_eq!(ago(100, 110), "à l'instant");
        assert_eq!(ago(0, 3 * 3600 + 5), "il y a 3 h");
    }
}
