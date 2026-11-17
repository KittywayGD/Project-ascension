//! Options de lancement : `cargo dev -- --seed 42 --demo --screenshot shots/vue.png`.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::world::generation::WorldSeed;
use crate::world::noise::SplitMix64;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchOptions {
    /// Seed demandée ; `None` = seed aléatoire.
    pub seed: Option<WorldSeed>,
    /// Construire la scène de démonstration au démarrage.
    pub demo: bool,
    /// Nom du monde lancé en ligne de commande : il obtient un emplacement de sauvegarde
    /// (sauvegarde automatique et en quittant). Sans nom, la partie n'est pas sauvegardée.
    pub name: Option<String>,
    /// Emplacement à charger directement, sans passer par l'écran titre.
    pub load: Option<String>,
    /// Outil de développement : prendre quelques captures fixes puis quitter.
    pub screenshot: Option<PathBuf>,
    /// Outil de développement : capture de l'écran titre puis fermeture.
    pub title_screenshot: Option<PathBuf>,
}

pub const USAGE: &str = "Usage : cargo dev -- [--seed <entier>] [--demo] [--name <nom>] \
[--load <emplacement>] [--screenshot <fichier.png>] [--title-screenshot <fichier.png>]";

impl LaunchOptions {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            // `--seed=42` équivaut à `--seed 42`.
            let (flag, inline) = match arg.split_once('=') {
                Some((flag, value)) if flag.starts_with("--") => {
                    (flag.to_string(), Some(value.to_string()))
                }
                _ => (arg.clone(), None),
            };
            let mut value = |example: &str| {
                inline.clone().or_else(|| args.next()).ok_or(format!(
                    "{flag} attend une valeur, par exemple {flag} {example}"
                ))
            };
            match flag.as_str() {
                "--demo" => options.demo = true,
                "--seed" => {
                    let value = value("42")?;
                    let parsed = value.parse::<u64>().map_err(|_| {
                        format!("seed invalide : « {value} » (entier positif attendu)")
                    })?;
                    options.seed = Some(WorldSeed(parsed));
                }
                "--name" => options.name = Some(value("\"Mon île\"")?),
                "--load" => options.load = Some(value("mon-ile")?),
                "--screenshot" => options.screenshot = Some(PathBuf::from(value("vue.png")?)),
                "--title-screenshot" => {
                    options.title_screenshot = Some(PathBuf::from(value("titre.png")?));
                }
                _ => return Err(format!("option inconnue : {arg}")),
            }
        }
        if options.load.is_some() && (options.seed.is_some() || options.demo) {
            return Err("--load reprend un monde existant : pas de --seed ni de --demo".into());
        }
        Ok(options)
    }

    /// Une option demande de jouer tout de suite (sans passer par l'écran titre).
    pub fn skips_title(&self) -> bool {
        self.seed.is_some()
            || self.demo
            || self.name.is_some()
            || self.load.is_some()
            || self.screenshot.is_some()
    }

    /// La seed demandée, ou une seed aléatoire tirée de l'horloge.
    pub fn seed_or_random(&self) -> WorldSeed {
        self.seed.unwrap_or_else(random_seed)
    }
}

pub fn random_seed() -> WorldSeed {
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
        assert_eq!(parse(&[]), Ok(LaunchOptions::default()));
    }

    #[test]
    fn demo_flag_combines_with_the_seed() {
        assert_eq!(
            parse(&["--demo", "--seed", "7"]),
            Ok(LaunchOptions {
                seed: Some(WorldSeed(7)),
                demo: true,
                ..LaunchOptions::default()
            })
        );
    }

    #[test]
    fn accepts_both_seed_syntaxes() {
        let expected = Ok(LaunchOptions {
            seed: Some(WorldSeed(42)),
            ..LaunchOptions::default()
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
        assert!(parse(&["--screenshot"]).is_err());
    }

    #[test]
    fn name_and_load_options() {
        let options = parse(&["--name", "Île test", "--seed=3"]).unwrap();
        assert_eq!(options.name.as_deref(), Some("Île test"));
        assert!(options.skips_title());
        assert_eq!(
            parse(&["--load", "ile"]).unwrap().load.as_deref(),
            Some("ile")
        );
        assert!(parse(&["--load", "ile", "--demo"]).is_err());
        assert!(!parse(&[]).unwrap().skips_title());
    }

    #[test]
    fn screenshot_takes_a_path() {
        let options = parse(&["--screenshot", "out/vue.png", "--demo"]).unwrap();
        assert_eq!(options.screenshot, Some(PathBuf::from("out/vue.png")));
        assert!(options.demo);
    }
}
