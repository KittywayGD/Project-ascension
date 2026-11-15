//! Options de lancement : `cargo dev -- --seed 42 --demo`.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::world::generation::WorldSeed;
use crate::world::noise::SplitMix64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchOptions {
    /// Seed demandée ; `None` = seed aléatoire.
    pub seed: Option<WorldSeed>,
    /// Construire la scène de démonstration au démarrage.
    pub demo: bool,
}

impl LaunchOptions {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut seed = None;
        let mut demo = false;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if arg == "--demo" {
                demo = true;
                continue;
            }
            let value = match arg.strip_prefix("--seed=") {
                Some(value) => value.to_string(),
                None if arg == "--seed" => args
                    .next()
                    .ok_or("--seed attend une valeur, par exemple --seed 42")?,
                None => return Err(format!("option inconnue : {arg}")),
            };
            let parsed = value
                .parse::<u64>()
                .map_err(|_| format!("seed invalide : « {value} » (entier positif attendu)"))?;
            seed = Some(WorldSeed(parsed));
        }
        Ok(Self { seed, demo })
    }

    /// La seed demandée, ou une seed aléatoire tirée de l'horloge.
    pub fn seed_or_random(&self) -> WorldSeed {
        self.seed.unwrap_or_else(random_seed)
    }
}

fn random_seed() -> WorldSeed {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    // Mélange pour que deux lancements rapprochés donnent des seeds très différentes.
    let mut rng = SplitMix64::new(nanos ^ u64::from(std::process::id()));
    // Seeds limitées à 9 chiffres : faciles à noter et à retaper.
    WorldSeed(rng.next_u64() % 1_000_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<LaunchOptions, String> {
        LaunchOptions::parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn no_arguments_means_random_seed() {
        assert_eq!(
            parse(&[]),
            Ok(LaunchOptions {
                seed: None,
                demo: false
            })
        );
    }

    #[test]
    fn demo_flag_combines_with_the_seed() {
        assert_eq!(
            parse(&["--demo", "--seed", "7"]),
            Ok(LaunchOptions {
                seed: Some(WorldSeed(7)),
                demo: true
            })
        );
    }

    #[test]
    fn accepts_both_seed_syntaxes() {
        let expected = Ok(LaunchOptions {
            seed: Some(WorldSeed(42)),
            demo: false,
        });
        assert_eq!(parse(&["--seed", "42"]), expected);
        assert_eq!(parse(&["--seed=42"]), expected);
    }

    #[test]
    fn rejects_invalid_input() {
        assert!(parse(&["--seed"]).is_err());
        assert!(parse(&["--seed", "abc"]).is_err());
        assert!(parse(&["--seed", "-3"]).is_err());
        assert!(parse(&["--plop"]).is_err());
    }
}
