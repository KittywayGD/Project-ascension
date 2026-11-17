//! Options du joueur : contrôles, affichage, sauvegarde automatique.
//!
//! Enregistrées dans `settings.ron`, à côté du dossier des sauvegardes, et appliquées
//! dès qu'elles changent (fenêtre, caméra, soleil, interface).
//! Les bornes et pas de chaque réglage sont dans [`SettingKey`] (Rust pur, testé).

use std::fs;
use std::path::PathBuf;

use bevy::camera::Projection;
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder, DirectionalLightShadowMap};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode};
use serde::{Deserialize, Serialize};

use crate::persistence::save_dir;
use crate::player::Player;
use crate::world::sky::camera_bloom;

/// Qualité des ombres du soleil.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShadowQuality {
    Off,
    Low,
    High,
}

/// Réglages du joueur. Les champs absents d'un ancien fichier prennent la valeur par défaut.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Sensibilité de la souris, en % de la valeur de base.
    pub mouse_sensitivity: u32,
    pub invert_y: bool,
    /// Champ de vision vertical, en degrés.
    pub fov_deg: u32,
    pub fullscreen: bool,
    pub vsync: bool,
    pub shadows: ShadowQuality,
    /// Halo lumineux des voyants, du four et du soleil.
    pub bloom: bool,
    /// Distance d'affichage des bâtiments, en mètres (la simulation, elle, tourne partout).
    pub view_distance_m: u32,
    /// Taille de l'interface, en %.
    pub ui_scale_pct: u32,
    /// Intervalle de la sauvegarde automatique, en minutes (0 = désactivée).
    pub autosave_min: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mouse_sensitivity: 100,
            invert_y: false,
            fov_deg: 45,
            fullscreen: false,
            vsync: true,
            shadows: ShadowQuality::High,
            bloom: true,
            view_distance_m: 150,
            ui_scale_pct: 100,
            autosave_min: 2,
        }
    }
}

impl Settings {
    /// Distance au-delà de laquelle un bâtiment affiché est déchargé (marge anti-clignotement).
    pub fn view_unload_m(&self) -> f32 {
        self.view_distance_m as f32 + 20.0
    }
}

/// Valeurs proposées pour la sauvegarde automatique (minutes, 0 = jamais).
const AUTOSAVE_CHOICES: [u32; 5] = [0, 1, 2, 5, 10];

/// Un réglage du menu d'options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingKey {
    MouseSensitivity,
    InvertY,
    Fov,
    Fullscreen,
    Vsync,
    Shadows,
    Bloom,
    ViewDistance,
    UiScale,
    Autosave,
}

impl SettingKey {
    /// Sections du menu, dans l'ordre d'affichage.
    pub const SECTIONS: [(&'static str, &'static [SettingKey]); 3] = [
        (
            "Contrôles",
            &[
                SettingKey::MouseSensitivity,
                SettingKey::InvertY,
                SettingKey::Fov,
            ],
        ),
        (
            "Affichage",
            &[
                SettingKey::Fullscreen,
                SettingKey::Vsync,
                SettingKey::Shadows,
                SettingKey::Bloom,
                SettingKey::ViewDistance,
                SettingKey::UiScale,
            ],
        ),
        ("Partie", &[SettingKey::Autosave]),
    ];

    pub fn label(self) -> &'static str {
        match self {
            SettingKey::MouseSensitivity => "Sensibilité de la souris",
            SettingKey::InvertY => "Inverser l'axe vertical",
            SettingKey::Fov => "Champ de vision",
            SettingKey::Fullscreen => "Plein écran",
            SettingKey::Vsync => "Synchronisation verticale",
            SettingKey::Shadows => "Ombres",
            SettingKey::Bloom => "Halo lumineux",
            SettingKey::ViewDistance => "Distance d'affichage des bâtiments",
            SettingKey::UiScale => "Taille de l'interface",
            SettingKey::Autosave => "Sauvegarde automatique",
        }
    }

    pub fn value(self, s: &Settings) -> String {
        let on_off = |on: bool| if on { "oui" } else { "non" }.to_string();
        match self {
            SettingKey::MouseSensitivity => format!("{} %", s.mouse_sensitivity),
            SettingKey::InvertY => on_off(s.invert_y),
            SettingKey::Fov => format!("{}°", s.fov_deg),
            SettingKey::Fullscreen => on_off(s.fullscreen),
            SettingKey::Vsync => on_off(s.vsync),
            SettingKey::Shadows => match s.shadows {
                ShadowQuality::Off => "désactivées",
                ShadowQuality::Low => "basses",
                ShadowQuality::High => "hautes",
            }
            .into(),
            SettingKey::Bloom => on_off(s.bloom),
            SettingKey::ViewDistance => format!("{} m", s.view_distance_m),
            SettingKey::UiScale => format!("{} %", s.ui_scale_pct),
            SettingKey::Autosave => match s.autosave_min {
                0 => "jamais".into(),
                1 => "toutes les minutes".into(),
                n => format!("toutes les {n} min"),
            },
        }
    }

    /// Change le réglage d'un cran (`direction` -1 ou +1), dans ses bornes.
    /// Les oui/non basculent quel que soit le sens.
    pub fn step(self, s: &mut Settings, direction: i32) {
        let up = direction > 0;
        let clamp_step = |value: u32, step: u32, min: u32, max: u32| {
            if up {
                (value + step).min(max)
            } else {
                value.saturating_sub(step).max(min)
            }
        };
        match self {
            SettingKey::MouseSensitivity => {
                s.mouse_sensitivity = clamp_step(s.mouse_sensitivity, 10, 20, 300);
            }
            SettingKey::InvertY => s.invert_y = !s.invert_y,
            SettingKey::Fov => s.fov_deg = clamp_step(s.fov_deg, 5, 40, 100),
            SettingKey::Fullscreen => s.fullscreen = !s.fullscreen,
            SettingKey::Vsync => s.vsync = !s.vsync,
            SettingKey::Shadows => {
                s.shadows = match (s.shadows, up) {
                    (ShadowQuality::Off, true) | (ShadowQuality::High, false) => ShadowQuality::Low,
                    (ShadowQuality::Low, true) | (ShadowQuality::High, true) => ShadowQuality::High,
                    (ShadowQuality::Low, false) | (ShadowQuality::Off, false) => ShadowQuality::Off,
                };
            }
            SettingKey::Bloom => s.bloom = !s.bloom,
            SettingKey::ViewDistance => {
                s.view_distance_m = clamp_step(s.view_distance_m, 25, 75, 400);
            }
            SettingKey::UiScale => s.ui_scale_pct = clamp_step(s.ui_scale_pct, 10, 70, 160),
            SettingKey::Autosave => {
                let index = AUTOSAVE_CHOICES
                    .iter()
                    .position(|&m| m >= s.autosave_min)
                    .unwrap_or(AUTOSAVE_CHOICES.len() - 1);
                let index = if up {
                    (index + 1).min(AUTOSAVE_CHOICES.len() - 1)
                } else {
                    index.saturating_sub(1)
                };
                s.autosave_min = AUTOSAVE_CHOICES[index];
            }
        }
    }

    /// Réglage à deux états (une seule flèche suffit).
    pub fn is_toggle(self) -> bool {
        matches!(
            self,
            SettingKey::InvertY | SettingKey::Fullscreen | SettingKey::Vsync | SettingKey::Bloom
        )
    }
}

/// Le menu d'options est ouvert (depuis la pause ou l'écran titre).
#[derive(Resource, Debug, Default)]
pub struct OptionsOpen(pub bool);

/// `settings.ron`, à côté du dossier `saves`.
fn settings_path() -> PathBuf {
    let saves = save_dir();
    saves
        .parent()
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
        .join("settings.ron")
}

fn load_settings() -> Settings {
    let path = settings_path();
    let Ok(text) = fs::read_to_string(&path) else {
        return Settings::default();
    };
    ron::from_str(&text).unwrap_or_else(|error| {
        warn!("Options illisibles ({}) : valeurs par défaut", error);
        Settings::default()
    })
}

/// Écrit les options (fichier temporaire puis remplacement).
fn store_settings(settings: &Settings) -> Result<(), String> {
    let path = settings_path();
    let text = ron::ser::to_string_pretty(settings, ron::ser::PrettyConfig::new())
        .map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("ron.tmp");
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(load_settings())
            .init_resource::<OptionsOpen>()
            .add_systems(
                Update,
                (
                    (
                        apply_window,
                        apply_ui_scale,
                        apply_shadows,
                        persist_settings,
                    )
                        .run_if(resource_changed::<Settings>),
                    apply_camera,
                ),
            );
    }
}

fn persist_settings(settings: Res<Settings>) {
    // Pas d'écriture au démarrage : seulement quand le joueur change quelque chose.
    if settings.is_added() {
        return;
    }
    if let Err(error) = store_settings(&settings) {
        warn!("Options non enregistrées : {error}");
    }
}

fn apply_window(settings: Res<Settings>, mut window: Single<&mut Window, With<PrimaryWindow>>) {
    let mode = if settings.fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    };
    if window.mode != mode {
        window.mode = mode;
    }
    let present = if settings.vsync {
        PresentMode::AutoVsync
    } else {
        PresentMode::AutoNoVsync
    };
    if window.present_mode != present {
        window.present_mode = present;
    }
}

fn apply_ui_scale(settings: Res<Settings>, mut scale: ResMut<UiScale>) {
    let wanted = settings.ui_scale_pct as f32 / 100.0;
    if (scale.0 - wanted).abs() > f32::EPSILON {
        scale.0 = wanted;
    }
}

fn apply_shadows(
    settings: Res<Settings>,
    mut map: ResMut<DirectionalLightShadowMap>,
    mut suns: Query<(&mut DirectionalLight, &mut CascadeShadowConfig)>,
) {
    let (enabled, size, cascades, distance) = match settings.shadows {
        ShadowQuality::Off => (false, 1024, 1, 50.0),
        ShadowQuality::Low => (true, 1024, 2, 90.0),
        ShadowQuality::High => (true, 2048, 4, 220.0),
    };
    if map.size != size {
        map.size = size;
    }
    for (mut light, mut config) in &mut suns {
        if light.shadow_maps_enabled != enabled {
            light.shadow_maps_enabled = enabled;
        }
        *config = CascadeShadowConfigBuilder {
            num_cascades: cascades,
            first_cascade_far_bound: 12.0_f32.min(distance / 2.0),
            maximum_distance: distance,
            ..default()
        }
        .build();
    }
}

/// Champ de vision et halo de la caméra du joueur (elle est recréée à chaque partie).
fn apply_camera(
    settings: Res<Settings>,
    mut cameras: Query<(Entity, &mut Projection, Has<Bloom>), With<Player>>,
    mut commands: Commands,
) {
    for (entity, mut projection, has_bloom) in &mut cameras {
        let fov = (settings.fov_deg as f32).to_radians();
        if let Projection::Perspective(perspective) = projection.as_ref()
            && (perspective.fov - fov).abs() > 1e-4
            && let Projection::Perspective(perspective) = projection.as_mut()
        {
            perspective.fov = fov;
        }
        if settings.bloom && !has_bloom {
            commands.entity(entity).insert(camera_bloom());
        } else if !settings.bloom && has_bloom {
            commands.entity(entity).remove::<Bloom>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_stay_within_bounds() {
        let mut s = Settings::default();
        for _ in 0..100 {
            SettingKey::Fov.step(&mut s, 1);
            SettingKey::MouseSensitivity.step(&mut s, -1);
        }
        assert_eq!(s.fov_deg, 100);
        assert_eq!(s.mouse_sensitivity, 20);
        SettingKey::Fov.step(&mut s, -1);
        assert_eq!(s.fov_deg, 95);
    }

    #[test]
    fn shadows_and_autosave_cycle_through_their_choices() {
        let mut s = Settings::default();
        SettingKey::Shadows.step(&mut s, -1);
        assert_eq!(s.shadows, ShadowQuality::Low);
        SettingKey::Shadows.step(&mut s, -1);
        SettingKey::Shadows.step(&mut s, -1);
        assert_eq!(s.shadows, ShadowQuality::Off);
        assert_eq!(SettingKey::Shadows.value(&s), "désactivées");

        assert_eq!(s.autosave_min, 2);
        SettingKey::Autosave.step(&mut s, 1);
        assert_eq!(s.autosave_min, 5);
        for _ in 0..5 {
            SettingKey::Autosave.step(&mut s, -1);
        }
        assert_eq!(SettingKey::Autosave.value(&s), "jamais");
    }

    #[test]
    fn toggles_flip_whatever_the_direction() {
        let mut s = Settings::default();
        SettingKey::InvertY.step(&mut s, -1);
        assert!(s.invert_y);
        SettingKey::InvertY.step(&mut s, 1);
        assert!(!s.invert_y);
    }

    #[test]
    fn old_files_missing_fields_get_defaults() {
        let s: Settings = ron::from_str("(fov_deg: 80)").unwrap();
        assert_eq!(s.fov_deg, 80);
        assert_eq!(s.view_distance_m, Settings::default().view_distance_m);
        let text = ron::ser::to_string(&s).unwrap();
        assert_eq!(ron::from_str::<Settings>(&text).unwrap(), s);
    }

    #[test]
    fn every_key_is_in_a_section() {
        let count: usize = SettingKey::SECTIONS
            .iter()
            .map(|(_, keys)| keys.len())
            .sum();
        assert_eq!(count, 10);
    }
}
