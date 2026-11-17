//! Ciel et éclairage : atmosphère physique, soleil, exposition, bloom et brume.
//!
//! L'éclairage est **physique** : soleil à l'éclairement réel (~130 000 lux) et caméra
//! exposée en conséquence. Conséquence pour les matériaux : une émission (voyant, four)
//! doit être multipliée par [`GLOW`] pour rester visible en plein jour.

use bevy::camera::Exposure;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::atmosphere::ScatteringMedium;
use bevy::light::light_consts::lux;
use bevy::light::{Atmosphere, AtmosphereEnvironmentMapLight, CascadeShadowConfigBuilder, SunDisk};
use bevy::pbr::{AtmosphereSettings, DistanceFog, FogFalloff};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;

use crate::game::Persistent;

/// Exposition de la caméra (EV100). Plus haut = image plus sombre.
const EXPOSURE_EV100: f32 = 13.0;

/// Facteur appliqué aux émissions des matériaux pour qu'elles ressortent sous
/// l'exposition « plein soleil » (et déclenchent le bloom).
pub const GLOW: f32 = 12.0;

/// Couleur de la brume lointaine, proche du bas du ciel.
pub const HAZE_COLOR: Color = Color::srgb(0.72, 0.80, 0.90);

/// Hauteur angulaire du soleil au-dessus de l'horizon, en degrés : assez bas pour des
/// ombres longues qui détachent les volumes, assez haut pour ne pas tout assombrir.
const SUN_ELEVATION_DEG: f32 = 38.0;
/// Direction d'où vient le soleil (0° = nord, -Z), en degrés.
const SUN_AZIMUTH_DEG: f32 = 215.0;

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        // Le ciel est dessiné par l'atmosphère ; le fond ne se voit qu'en cas de souci.
        app.insert_resource(ClearColor(HAZE_COLOR))
            // L'ambiance vient du ciel (`AtmosphereEnvironmentMapLight`) : on garde juste
            // un léger plancher pour que les faces à l'ombre ne soient jamais noires.
            .insert_resource(GlobalAmbientLight {
                color: Color::WHITE,
                brightness: 1500.0,
                ..default()
            })
            .add_systems(Startup, (spawn_atmosphere, spawn_sun));
    }
}

/// Réglages de rendu de la caméra du joueur (HDR, ciel, exposition, bloom, brume).
pub fn camera_rendering() -> impl Bundle {
    (
        AtmosphereSettings::default(),
        AtmosphereEnvironmentMapLight::default(),
        Exposure {
            ev100: EXPOSURE_EV100,
        },
        Tonemapping::TonyMcMapface,
        camera_bloom(),
        // Brume légère qui donne de la profondeur ; l'atmosphère fait le reste au loin.
        DistanceFog {
            color: HAZE_COLOR,
            directional_light_color: Color::srgba(1.0, 0.92, 0.75, 0.5),
            directional_light_exponent: 24.0,
            falloff: FogFalloff::from_visibility_colors(
                2600.0,
                Color::srgb(0.30, 0.35, 0.42),
                Color::srgb(0.85, 0.88, 0.92),
            ),
        },
    )
}

/// Bloom discret : fait briller voyants, four et modules sans baver (désactivable).
pub fn camera_bloom() -> Bloom {
    Bloom {
        intensity: 0.12,
        ..Bloom::NATURAL
    }
}

fn spawn_atmosphere(mut commands: Commands, mut mediums: ResMut<Assets<ScatteringMedium>>) {
    let earth = mediums.add(ScatteringMedium::earth(256, 256));
    commands.spawn((
        Name::new("Atmosphère"),
        Atmosphere::earth(earth),
        Persistent,
    ));
}

/// Direction de la lumière du soleil (du soleil vers le sol).
pub fn sun_direction() -> Vec3 {
    let elevation = SUN_ELEVATION_DEG.to_radians();
    let azimuth = SUN_AZIMUTH_DEG.to_radians();
    let towards_sun = Vec3::new(
        azimuth.sin() * elevation.cos(),
        elevation.sin(),
        -azimuth.cos() * elevation.cos(),
    );
    -towards_sun
}

fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        Name::new("Soleil"),
        Persistent,
        DirectionalLight {
            illuminance: lux::RAW_SUNLIGHT,
            shadow_maps_enabled: true,
            ..default()
        },
        SunDisk::EARTH,
        // Ombres nettes près du joueur, jusqu'à la distance d'affichage des bâtiments.
        CascadeShadowConfigBuilder {
            num_cascades: 4,
            first_cascade_far_bound: 12.0,
            maximum_distance: 220.0,
            ..default()
        }
        .build(),
        Transform::default().looking_to(sun_direction(), Vec3::Y),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sun_shines_downwards_at_the_chosen_elevation() {
        let dir = sun_direction();
        assert!((dir.length() - 1.0).abs() < 1e-5);
        let elevation = (-dir.y).asin().to_degrees();
        assert!((elevation - SUN_ELEVATION_DEG).abs() < 1e-3);
    }
}
