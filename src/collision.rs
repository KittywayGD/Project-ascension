//! Collisions simples, sans moteur physique : le joueur est un cylindre vertical,
//! les obstacles sont des cercles (rochers) ou des rectangles (bâtiments) avec un sommet.
//!
//! Un obstacle dont le sommet dépasse les pieds de moins de `step_up` est franchi
//! (on monte dessus) ; un obstacle plus haut repousse horizontalement.

use bevy::math::Vec3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Obstacle {
    Circle {
        x: f32,
        z: f32,
        radius: f32,
        top: f32,
    },
    /// Rectangle aligné sur les axes (les bâtiments tournent par quarts de tour).
    Rect {
        min_x: f32,
        max_x: f32,
        min_z: f32,
        max_z: f32,
        top: f32,
    },
}

impl Obstacle {
    pub fn top(&self) -> f32 {
        match *self {
            Obstacle::Circle { top, .. } | Obstacle::Rect { top, .. } => top,
        }
    }

    /// Vecteur (dx, dz) du point le plus proche de l'obstacle vers (x, z), et sa longueur.
    /// Longueur nulle si le point est à l'intérieur d'un rectangle.
    fn separation(&self, x: f32, z: f32) -> (f32, f32, f32) {
        match *self {
            Obstacle::Circle {
                x: cx,
                z: cz,
                radius,
                ..
            } => {
                let (dx, dz) = (x - cx, z - cz);
                let distance = (dx * dx + dz * dz).sqrt();
                // Distance au bord du cercle (négative à l'intérieur).
                (dx, dz, distance - radius)
            }
            Obstacle::Rect {
                min_x,
                max_x,
                min_z,
                max_z,
                ..
            } => {
                let (dx, dz) = (x - x.clamp(min_x, max_x), z - z.clamp(min_z, max_z));
                (dx, dz, (dx * dx + dz * dz).sqrt())
            }
        }
    }

    /// Le cercle de centre (x, z) et de rayon `radius` touche-t-il l'obstacle ?
    pub fn overlaps_circle(&self, x: f32, z: f32, radius: f32) -> bool {
        self.separation(x, z).2 < radius
    }

    /// Position la plus proche où un cercle de rayon `radius` ne touche plus l'obstacle.
    fn push_out(&self, x: f32, z: f32, radius: f32) -> (f32, f32) {
        let (dx, dz, gap) = self.separation(x, z);
        let length = (dx * dx + dz * dz).sqrt();
        if length > 1e-4 {
            let push = radius - gap;
            return (x + dx / length * push, z + dz / length * push);
        }
        match *self {
            // Au centre exact d'un cercle : direction arbitraire mais fixe.
            Obstacle::Circle {
                x: cx,
                z: cz,
                radius: r,
                ..
            } => (cx + r + radius, cz),
            // À l'intérieur d'un rectangle : sortie par le bord le plus proche.
            Obstacle::Rect {
                min_x,
                max_x,
                min_z,
                max_z,
                ..
            } => {
                let exits = [
                    (x - min_x, (min_x - radius, z)),
                    (max_x - x, (max_x + radius, z)),
                    (z - min_z, (x, min_z - radius)),
                    (max_z - z, (x, max_z + radius)),
                ];
                exits
                    .into_iter()
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|(_, p)| p)
                    .unwrap_or((x, z))
            }
        }
    }
}

/// Résultat d'une collision horizontale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Collision {
    /// Position horizontale corrigée.
    pub x: f32,
    pub z: f32,
    /// Sommet du plus haut obstacle sur lequel on peut se tenir ici, s'il y en a un.
    pub support: Option<f32>,
}

/// Résout la collision d'un corps vertical (rayon `radius`, pieds à `feet`).
pub fn collide(
    x: f32,
    z: f32,
    feet: f32,
    radius: f32,
    step_up: f32,
    obstacles: impl IntoIterator<Item = Obstacle>,
) -> Collision {
    let (mut x, mut z) = (x, z);
    let mut support: Option<f32> = None;
    for obstacle in obstacles {
        if !obstacle.overlaps_circle(x, z, radius) {
            continue;
        }
        let top = obstacle.top();
        if feet >= top - step_up {
            support = Some(support.map_or(top, |s| s.max(top)));
        } else {
            (x, z) = obstacle.push_out(x, z, radius);
        }
    }
    Collision { x, z, support }
}

/// Sommet le plus haut parmi les obstacles qui touchent un cercle (vol : on glisse dessus).
pub fn highest_top(
    x: f32,
    z: f32,
    radius: f32,
    obstacles: impl IntoIterator<Item = Obstacle>,
) -> Option<f32> {
    obstacles
        .into_iter()
        .filter(|o| o.overlaps_circle(x, z, radius))
        .map(|o| o.top())
        .reduce(f32::max)
}

/// Distance d'entrée d'un rayon dans une boîte alignée sur les axes (méthode des dalles).
pub fn ray_aabb(origin: Vec3, direction: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let inverse = direction.recip();
    let t1 = (min - origin) * inverse;
    let t2 = (max - origin) * inverse;
    let near = t1.min(t2).max_element();
    let far = t1.max(t2).min_element();
    (far >= near.max(0.0)).then_some(near.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROCK: Obstacle = Obstacle::Circle {
        x: 0.0,
        z: 0.0,
        radius: 1.0,
        top: 2.0,
    };
    const BUILDING: Obstacle = Obstacle::Rect {
        min_x: -1.5,
        max_x: 1.5,
        min_z: -1.5,
        max_z: 1.5,
        top: 3.0,
    };

    #[test]
    fn tall_circle_pushes_out() {
        let hit = collide(0.5, 0.0, 0.0, 0.3, 0.45, [ROCK]);
        assert!((hit.x - 1.3).abs() < 1e-5 && hit.z == 0.0, "{hit:?}");
        assert_eq!(hit.support, None);
    }

    #[test]
    fn low_obstacle_is_stepped_onto() {
        // Pieds à 1,7 m : le sommet (2 m) n'est qu'à 30 cm au-dessus.
        let hit = collide(0.5, 0.0, 1.7, 0.3, 0.45, [ROCK]);
        assert_eq!((hit.x, hit.z, hit.support), (0.5, 0.0, Some(2.0)));
    }

    #[test]
    fn rect_pushes_out_from_its_side() {
        let hit = collide(1.6, 0.2, 0.0, 0.3, 0.45, [BUILDING]);
        assert!(
            (hit.x - 1.8).abs() < 1e-5 && (hit.z - 0.2).abs() < 1e-5,
            "{hit:?}"
        );
    }

    #[test]
    fn rect_pushes_out_through_the_nearest_edge_from_inside() {
        let hit = collide(1.2, 0.0, 0.0, 0.3, 0.45, [BUILDING]);
        assert!((hit.x - 1.8).abs() < 1e-5, "{hit:?}");
    }

    #[test]
    fn far_obstacles_are_ignored() {
        let hit = collide(5.0, 5.0, 0.0, 0.3, 0.45, [ROCK, BUILDING]);
        assert_eq!((hit.x, hit.z, hit.support), (5.0, 5.0, None));
        assert_eq!(highest_top(5.0, 5.0, 0.3, [ROCK, BUILDING]), None);
        assert_eq!(highest_top(1.0, 1.0, 0.3, [ROCK, BUILDING]), Some(3.0));
    }

    #[test]
    fn ray_hits_box_front_face() {
        let min = Vec3::new(-1.0, 0.0, -1.0);
        let max = Vec3::new(1.0, 2.0, 1.0);
        let origin = Vec3::new(-5.0, 1.0, 0.0);
        assert_eq!(ray_aabb(origin, Vec3::X, min, max), Some(4.0));
        assert_eq!(ray_aabb(origin, Vec3::NEG_X, min, max), None);
        assert_eq!(ray_aabb(Vec3::new(-5.0, 3.0, 0.0), Vec3::X, min, max), None);
    }
}
