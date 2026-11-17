//! Captures d'écran : F12 en jeu, et `--screenshot <fichier.png>` pour l'outil de
//! développement qui prend quelques vues fixes puis quitte (utile pour juger un
//! changement visuel sans jouer).
//!
//! `--screenshot vue.png` produit `vue-usine.png`, `vue-detail.png`, `vue-paysage.png`,
//! `vue-pause.png` (menu pause), `vue-construction.png` (menu de construction) et
//! `vue-options.png` (options).
//! Les vues visent le centre des bâtiments posés (lancer avec `--demo`), sinon le joueur.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::transform::TransformSystems;

use crate::app::PlayState;
use crate::building::Buildings;
use crate::game::Screen;
use crate::player::{MoveMode, Player};
use crate::settings::OptionsOpen;
use crate::ui::OpenMenu;
use crate::world::Terrain;

/// Dossier des captures F12 (ignoré par Git).
const SCREENSHOT_DIR: &str = "screenshots";
/// Frames laissées au monde avant la première vue (chargement, usine qui démarre).
const WARMUP_FRAMES: u32 = 240;
/// Frames entre deux vues (ombres et exposition stabilisées).
const FRAMES_PER_VIEW: u32 = 45;

/// Chemin demandé par `--screenshot` ; `None` en partie normale.
#[derive(Resource, Debug, Clone, Default)]
pub struct CaptureRequest(pub Option<PathBuf>);

#[derive(Debug, Clone, Copy)]
enum View {
    /// Vue plongeante d'ensemble sur l'usine.
    Factory,
    /// Plus près, à hauteur d'homme : voyants, ports, modules.
    Detail,
    /// Vue haute et lointaine : relief, mer, brouillard, ciel.
    Landscape,
    /// Menu pause par-dessus l'usine.
    Pause,
    /// Menu de construction (B) ouvert.
    BuildMenu,
    /// Options ouvertes depuis le menu pause.
    Options,
}

impl View {
    /// État de la partie pendant cette vue.
    fn play_state(self) -> PlayState {
        match self {
            View::Pause | View::Options => PlayState::Paused,
            View::BuildMenu => PlayState::Menu,
            View::Factory | View::Detail | View::Landscape => PlayState::Running,
        }
    }
}

const VIEWS: [(View, &str); 6] = [
    (View::Factory, "usine"),
    (View::Detail, "detail"),
    (View::Landscape, "paysage"),
    (View::Pause, "pause"),
    (View::BuildMenu, "construction"),
    (View::Options, "options"),
];

/// Chemin demandé par `--title-screenshot`.
#[derive(Resource, Debug, Clone, Default)]
pub struct TitleCaptureRequest(pub Option<PathBuf>);

#[derive(Resource, Debug, Default)]
struct CaptureRun {
    frame: u32,
    saved: usize,
}

pub struct CapturePlugin;

impl Plugin for CapturePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CaptureRequest>()
            .init_resource::<TitleCaptureRequest>()
            .add_systems(
                Update,
                capture_title
                    .run_if(|request: Res<TitleCaptureRequest>| request.0.is_some())
                    .run_if(in_state(Screen::Title)),
            )
            .init_resource::<CaptureRun>()
            .add_systems(Update, screenshot_on_f12)
            .add_systems(
                PostUpdate,
                auto_capture
                    .before(TransformSystems::Propagate)
                    .run_if(|request: Res<CaptureRequest>| request.0.is_some())
                    .run_if(in_state(Screen::InGame)),
            );
    }
}

fn screenshot_on_f12(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if !keys.just_pressed(KeyCode::F12) {
        return;
    }
    if let Err(error) = std::fs::create_dir_all(SCREENSHOT_DIR) {
        warn!("Capture impossible : {error}");
        return;
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let path = Path::new(SCREENSHOT_DIR).join(format!("ascension-{stamp}.png"));
    info!("Capture : {}", path.display());
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

/// Une capture de l'écran titre après quelques frames, puis fermeture.
fn capture_title(
    request: Res<TitleCaptureRequest>,
    mut frame: Local<u32>,
    mut saved: Local<bool>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(path) = &request.0 else { return };
    *frame += 1;
    if *frame == 60 {
        info!("Capture de l'écran titre : {}", path.display());
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path.clone()));
        *saved = true;
    }
    if *saved && *frame > 90 {
        exit.write(AppExit::Success);
    }
}

/// `vue.png` + `usine` → `vue-usine.png`.
fn view_path(base: &Path, suffix: &str) -> PathBuf {
    let stem = base
        .file_stem()
        .map_or("capture".into(), |s| s.to_string_lossy());
    let ext = base
        .extension()
        .map_or("png".into(), |s| s.to_string_lossy());
    base.with_file_name(format!("{stem}-{suffix}.{ext}"))
}

fn auto_capture(
    request: Res<CaptureRequest>,
    mut run: ResMut<CaptureRun>,
    state: Res<State<PlayState>>,
    mut next: ResMut<NextState<PlayState>>,
    terrain: Res<Terrain>,
    buildings: Res<Buildings>,
    player: Single<(&mut Transform, &mut Player)>,
    mut overlay: Query<&mut Visibility, With<super::DebugOverlay>>,
    mut menu: ResMut<OpenMenu>,
    mut options: ResMut<OptionsOpen>,
    diagnostics: Res<DiagnosticsStore>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(base) = &request.0 else { return };
    run.frame += 1;
    for mut visibility in &mut overlay {
        *visibility = Visibility::Hidden;
    }

    if run.saved >= VIEWS.len() {
        exit.write(AppExit::Success);
        return;
    }
    let Some(elapsed) = run.frame.checked_sub(WARMUP_FRAMES) else {
        if *state.get() != PlayState::Running {
            next.set(PlayState::Running);
        }
        return;
    };
    let index = (elapsed / FRAMES_PER_VIEW) as usize;
    let Some(&(view, suffix)) = VIEWS.get(index) else {
        return;
    };
    let wants_options = matches!(view, View::Options);
    if options.0 != wants_options {
        options.0 = wants_options;
    }
    let wanted = view.play_state();
    if *state.get() != wanted {
        if wanted == PlayState::Menu {
            *menu = OpenMenu::Build;
        }
        next.set(wanted);
    }

    let (mut transform, mut player) = player.into_inner();
    player.mode = MoveMode::Fly;
    *transform = camera_for(view, &terrain, &buildings, transform.translation);

    if elapsed % FRAMES_PER_VIEW == FRAMES_PER_VIEW - 1 {
        let path = view_path(base, suffix);
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(dir);
        }
        let fps = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FPS)
            .and_then(|d| d.smoothed())
            .unwrap_or(0.0);
        info!("Capture automatique : {} ({fps:.0} FPS)", path.display());
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path))
            .observe(|_: On<ScreenshotCaptured>, mut run: ResMut<CaptureRun>| {
                run.saved += 1;
            });
    }
}

/// Position et visée de la caméra pour une vue.
fn camera_for(view: View, terrain: &Terrain, buildings: &Buildings, fallback: Vec3) -> Transform {
    let field = &terrain.0;
    let ground = |x: f32, z: f32| field.height_at(x, z).max(0.0);
    let placed: Vec<Vec3> = buildings
        .0
        .iter()
        .map(|(_, b)| Vec3::new(b.x, b.base_y, b.z))
        .collect();
    let center = if placed.is_empty() {
        Vec3::new(fallback.x, ground(fallback.x, fallback.z), fallback.z)
    } else {
        placed.iter().sum::<Vec3>() / placed.len() as f32
    };

    let (offset, height, look_up) = match view {
        View::Factory | View::Pause | View::BuildMenu | View::Options => {
            (Vec3::new(16.0, 0.0, 16.0), 11.0, 0.5)
        }
        View::Detail => (Vec3::new(7.0, 0.0, -6.0), 3.2, 1.2),
        View::Landscape => (Vec3::new(-120.0, 0.0, -150.0), 70.0, 0.0),
    };
    let x = center.x + offset.x;
    let z = center.z + offset.z;
    let eye = Vec3::new(x, ground(x, z).max(center.y) + height, z);
    Transform::from_translation(eye).looking_at(center + Vec3::Y * look_up, Vec3::Y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_paths_keep_folder_and_extension() {
        assert_eq!(
            view_path(Path::new("out/vue.png"), "usine"),
            PathBuf::from("out/vue-usine.png")
        );
        assert_eq!(
            view_path(Path::new("vue"), "detail"),
            PathBuf::from("vue-detail.png")
        );
    }
}
