//! État global de la partie : en jeu / menu ouvert / pause, capture de la souris, fermeture.

use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow, WindowFocused};

use crate::building::BuildMode;

/// La partie démarre en pause : un clic capture la souris et lance le jeu.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PlayState {
    /// Tout est figé, simulation comprise.
    #[default]
    Paused,
    Running,
    /// Un menu (inventaire, machine) est ouvert : le joueur ne bouge plus et la souris
    /// est libre, mais **la simulation continue** (les usines tournent).
    Menu,
}

pub struct AppStatePlugin;

impl Plugin for AppStatePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<PlayState>()
            .add_systems(OnEnter(PlayState::Running), grab_cursor)
            .add_systems(
                OnTransition {
                    exited: PlayState::Paused,
                    entered: PlayState::Running,
                },
                log_resume,
            )
            .add_systems(OnEnter(PlayState::Paused), (release_cursor, log_pause))
            .add_systems(OnEnter(PlayState::Menu), release_cursor)
            .add_systems(
                Update,
                (
                    quit_on_shift_escape,
                    toggle_pause,
                    start_on_click.run_if(in_state(PlayState::Paused)),
                    pause_on_focus_lost.run_if(not(in_state(PlayState::Paused))),
                ),
            );
    }
}

fn grab_cursor(mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    // macOS ne gère que `Locked` (pas `Confined`) : c'est le mode attendu pour une caméra FPS.
    cursor.grab_mode = CursorGrabMode::Locked;
    cursor.visible = false;
}

fn log_resume() {
    info!("Partie reprise");
}

fn release_cursor(mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    cursor.grab_mode = CursorGrabMode::None;
    cursor.visible = true;
}

fn log_pause() {
    info!("Partie en pause");
}

fn shift_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight])
}

/// Échap ferme le menu ouvert, quitte la construction, sinon bascule pause / jeu
/// (Maj+Échap est réservé à la fermeture).
fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<PlayState>>,
    mut build: ResMut<BuildMode>,
    mut next: ResMut<NextState<PlayState>>,
) {
    if keys.just_pressed(KeyCode::Escape) && !shift_pressed(&keys) {
        if *state.get() == PlayState::Running && build.active {
            build.stop();
            info!("Construction arrêtée");
            return;
        }
        next.set(match state.get() {
            PlayState::Running => PlayState::Paused,
            PlayState::Paused | PlayState::Menu => PlayState::Running,
        });
    }
}

fn start_on_click(mouse: Res<ButtonInput<MouseButton>>, mut next: ResMut<NextState<PlayState>>) {
    if mouse.just_pressed(MouseButton::Left) {
        next.set(PlayState::Running);
    }
}

fn pause_on_focus_lost(
    mut focus: MessageReader<WindowFocused>,
    mut next: ResMut<NextState<PlayState>>,
) {
    if focus.read().any(|event| !event.focused) {
        next.set(PlayState::Paused);
    }
}

fn quit_on_shift_escape(keys: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) && shift_pressed(&keys) {
        info!("Fermeture demandée");
        exit.write(AppExit::Success);
    }
}
