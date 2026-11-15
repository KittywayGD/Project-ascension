//! Bruit de Perlin 2D déterministe, sans dépendance externe.
//!
//! Règle de déterminisme (voir `docs/worldgen.md`) : uniquement des opérations IEEE
//! exactes (`+`, `-`, `*`, `/`, `floor`) et des polynômes. Pas de `sin`, `exp`, `powf`…
//! dont le résultat peut varier selon la plateforme ou la libm.

/// Générateur pseudo-aléatoire SplitMix64 : petit, rapide, entièrement spécifié.
#[derive(Debug, Clone)]
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Flottant uniforme dans [0, 1), construit à partir des 53 bits de poids fort.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Bruit de Perlin 2D (gradients sur 8 directions), à valeurs dans environ [-1, 1].
#[derive(Debug, Clone)]
pub struct Perlin {
    perm: [u8; 512],
}

impl Perlin {
    pub fn new(seed: u64) -> Self {
        let mut table: [u8; 256] = std::array::from_fn(|i| i as u8);
        // Mélange de Fisher-Yates piloté par la seed.
        let mut rng = SplitMix64::new(seed);
        for i in (1..256).rev() {
            let j = (rng.next_u64() % (i as u64 + 1)) as usize;
            table.swap(i, j);
        }
        let perm = std::array::from_fn(|i| table[i & 255]);
        Self { perm }
    }

    pub fn sample(&self, x: f64, y: f64) -> f64 {
        let x0 = x.floor();
        let y0 = y.floor();
        let (fx, fy) = (x - x0, y - y0);
        // `rem_euclid` garde un indice positif pour les coordonnées négatives.
        let xi = (x0 as i64).rem_euclid(256) as usize;
        let yi = (y0 as i64).rem_euclid(256) as usize;

        let p = &self.perm;
        let aa = p[p[xi] as usize + yi];
        let ab = p[p[xi] as usize + yi + 1];
        let ba = p[p[xi + 1] as usize + yi];
        let bb = p[p[xi + 1] as usize + yi + 1];

        let u = fade(fx);
        let v = fade(fy);
        let bottom = lerp(grad(aa, fx, fy), grad(ba, fx - 1.0, fy), u);
        let top = lerp(grad(ab, fx, fy - 1.0), grad(bb, fx - 1.0, fy - 1.0), u);
        // Gradients de norme ≤ √2 : la valeur reste dans [-1, 1] (borne √(n/4) × √2 en 2D).
        lerp(bottom, top, v)
    }
}

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn grad(hash: u8, x: f64, y: f64) -> f64 {
    match hash & 7 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        3 => -x - y,
        4 => x,
        5 => -x,
        6 => y,
        _ => -y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix_is_reproducible() {
        let mut a = SplitMix64::new(7);
        let mut b = SplitMix64::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn same_seed_same_noise() {
        let a = Perlin::new(42);
        let b = Perlin::new(42);
        for i in 0..1000 {
            let (x, y) = (i as f64 * 0.37 - 100.0, i as f64 * 0.11 + 3.0);
            assert_eq!(a.sample(x, y).to_bits(), b.sample(x, y).to_bits());
        }
    }

    #[test]
    fn different_seeds_differ() {
        let a = Perlin::new(1);
        let b = Perlin::new(2);
        let differs = (0..100).any(|i| {
            let x = i as f64 * 0.53 + 0.25;
            a.sample(x, x * 0.7) != b.sample(x, x * 0.7)
        });
        assert!(differs);
    }

    #[test]
    fn zero_on_lattice_points_and_bounded_elsewhere() {
        let noise = Perlin::new(9);
        for i in -50..50 {
            assert_eq!(noise.sample(i as f64, (i * 3) as f64), 0.0);
        }
        for i in 0..10_000 {
            let v = noise.sample(i as f64 * 0.173, i as f64 * 0.0291 - 40.0);
            assert!((-1.0..=1.0).contains(&v), "{v}");
        }
    }
}
