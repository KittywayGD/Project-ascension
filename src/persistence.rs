//! Sauvegardes (V0.1), en Rust pur : format RON versionné, plusieurs emplacements nommés.
//!
//! Ce qui est sauvegardé : seed et version de génération (le monde est **régénéré**
//! à l'identique, pas stocké), simulation complète (bâtiments, convoyeurs, modules,
//! inventaires, progression des recettes, livraisons), bâtiments posés, rochers détruits,
//! joueur. Les réseaux électriques sont recalculés au chargement.
//!
//! Écriture **atomique** : fichier temporaire, puis l'ancienne sauvegarde devient la
//! copie de secours (`.bak`) et la nouvelle prend sa place. Une écriture interrompue ne
//! détruit donc jamais la dernière sauvegarde valide.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::building::placement::PlacedBuildings;
use crate::inventory::Inventory;
use crate::simulation::sim_world::SimWorld;
use crate::world::generation::{WORLDGEN_VERSION, WorldSeed};
use crate::world::rocks::RockId;

/// Version du format. À incrémenter (avec une migration) à chaque changement incompatible.
pub const SAVE_VERSION: u32 = 1;

/// Extension des emplacements de sauvegarde.
const EXTENSION: &str = "ron";
/// Variable d'environnement qui remplace le dossier des sauvegardes (tests, captures).
pub const SAVE_DIR_ENV: &str = "ASCENSION_SAVE_DIR";
/// Longueur maximale d'un nom de monde, en caractères.
pub const MAX_NAME_CHARS: usize = 32;

/// Informations d'en-tête, lisibles sans charger toute la partie.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveMeta {
    /// Nom du monde choisi par le joueur.
    pub name: String,
    pub seed: WorldSeed,
    /// Version de génération du monde : une autre version donnerait un autre relief.
    pub worldgen_version: u32,
    /// Mode créatif : construction gratuite, tout débloqué.
    pub creative: bool,
    /// Dates en secondes Unix.
    pub created_unix: u64,
    pub saved_unix: u64,
    /// Temps simulé total, en ticks.
    pub ticks: u64,
}

/// État du joueur.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerSave {
    pub position: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub flying: bool,
    pub inventory: Inventory,
}

/// Une partie complète.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveGame {
    pub version: u32,
    pub meta: SaveMeta,
    pub player: PlayerSave,
    pub sim: SimWorld,
    pub placed: PlacedBuildings,
    pub destroyed_rocks: BTreeSet<RockId>,
}

/// En-tête seul (les autres champs du fichier sont ignorés à la lecture).
#[derive(Debug, Clone, Deserialize)]
struct SaveHeader {
    version: u32,
    meta: SaveMeta,
}

/// Pourquoi une sauvegarde ne peut pas être lue ou écrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    Io(String),
    /// Fichier illisible (tronqué, modifié à la main…).
    Corrupt(String),
    /// Écrite par une version plus récente du jeu.
    TooNew(u32),
    /// Format plus ancien que l'on ne sait pas encore migrer.
    TooOld(u32),
    /// Monde généré par une autre version du générateur.
    WorldGenMismatch(u32),
}

impl SaveError {
    pub fn message(&self) -> String {
        match self {
            SaveError::Io(e) => format!("fichier inaccessible ({e})"),
            SaveError::Corrupt(e) => format!("sauvegarde endommagée ({e})"),
            SaveError::TooNew(v) => {
                format!(
                    "sauvegarde d'une version plus récente du jeu (format {v}, ce jeu lit le {SAVE_VERSION})"
                )
            }
            SaveError::TooOld(v) => format!("format {v} trop ancien, sans conversion disponible"),
            SaveError::WorldGenMismatch(v) => format!(
                "monde créé avec le générateur v{v}, ce jeu utilise la v{} : le relief serait différent",
                WORLDGEN_VERSION.0
            ),
        }
    }
}

impl From<std::io::Error> for SaveError {
    fn from(error: std::io::Error) -> Self {
        SaveError::Io(error.to_string())
    }
}

/// Un emplacement trouvé dans le dossier des sauvegardes.
#[derive(Debug, Clone)]
pub struct SlotInfo {
    /// Identifiant de fichier (nom sans extension).
    pub id: String,
    /// En-tête, ou la raison pour laquelle il est illisible.
    pub meta: Result<SaveMeta, SaveError>,
    /// Une copie de secours (sauvegarde précédente) existe.
    pub has_backup: bool,
}

/// Dossier des sauvegardes : `ASCENSION_SAVE_DIR`, sinon le dossier de données du système.
pub fn save_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(SAVE_DIR_ENV) {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Application Support/Project Ascension"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Project Ascension"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| home.map(|h| h.join(".local/share")))
            .map(|d| d.join("project-ascension"))
    };
    base.unwrap_or_else(|| PathBuf::from(".")).join("saves")
}

fn slot_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.{EXTENSION}"))
}

fn backup_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.{EXTENSION}.bak"))
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Nettoie un nom saisi : espaces superflus retirés ; `None` si vide ou trop long.
pub fn clean_name(name: &str) -> Option<String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    (!name.is_empty() && name.chars().count() <= MAX_NAME_CHARS).then_some(name)
}

/// Nom de fichier tiré du nom du monde : minuscules ASCII, chiffres et tirets.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { "monde".into() } else { out }
}

/// Identifiant libre pour un nouveau monde nommé `name` (`mon-monde`, `mon-monde-2`…).
pub fn new_slot_id(dir: &Path, name: &str) -> String {
    let base = slug(name);
    let taken = |id: &str| slot_path(dir, id).exists() || backup_path(dir, id).exists();
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|id| !taken(id))
        .expect("suite infinie")
}

/// Texte RON d'une partie.
pub fn to_ron(save: &SaveGame) -> Result<String, SaveError> {
    let config = ron::ser::PrettyConfig::new().depth_limit(4);
    ron::ser::to_string_pretty(save, config).map_err(|e| SaveError::Corrupt(e.to_string()))
}

/// Lit une partie depuis du texte RON, en vérifiant les versions.
pub fn from_ron(text: &str) -> Result<SaveGame, SaveError> {
    let header: SaveHeader = ron::from_str(text).map_err(|e| SaveError::Corrupt(e.to_string()))?;
    check_versions(header.version, &header.meta)?;
    let mut save: SaveGame = ron::from_str(text).map_err(|e| SaveError::Corrupt(e.to_string()))?;
    save.sim.after_load();
    Ok(save)
}

fn check_versions(version: u32, meta: &SaveMeta) -> Result<(), SaveError> {
    if version > SAVE_VERSION {
        return Err(SaveError::TooNew(version));
    }
    if version < SAVE_VERSION {
        return Err(SaveError::TooOld(version));
    }
    if meta.worldgen_version != WORLDGEN_VERSION.0 {
        return Err(SaveError::WorldGenMismatch(meta.worldgen_version));
    }
    Ok(())
}

/// Écrit atomiquement l'emplacement `id` ; l'ancienne version devient la copie de secours.
pub fn write_slot(dir: &Path, id: &str, save: &SaveGame) -> Result<(), SaveError> {
    let text = to_ron(save)?;
    fs::create_dir_all(dir)?;
    let path = slot_path(dir, id);
    let tmp = dir.join(format!("{id}.{EXTENSION}.tmp"));
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    }
    if path.exists() {
        fs::rename(&path, backup_path(dir, id))?;
    }
    fs::rename(&tmp, &path)?;
    Ok(())
}

/// Charge l'emplacement `id`.
pub fn read_slot(dir: &Path, id: &str) -> Result<SaveGame, SaveError> {
    from_ron(&fs::read_to_string(slot_path(dir, id))?)
}

/// Charge la copie de secours de l'emplacement `id`.
pub fn read_backup(dir: &Path, id: &str) -> Result<SaveGame, SaveError> {
    from_ron(&fs::read_to_string(backup_path(dir, id))?)
}

/// Supprime un emplacement et sa copie de secours.
pub fn delete_slot(dir: &Path, id: &str) -> Result<(), SaveError> {
    for path in [slot_path(dir, id), backup_path(dir, id)] {
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

/// Remplace une sauvegarde illisible par sa copie de secours. Le fichier endommagé est
/// gardé à côté (`.ron.corrupt`) pour pouvoir l'examiner.
pub fn restore_backup(dir: &Path, id: &str) -> Result<SaveGame, SaveError> {
    let save = read_backup(dir, id)?;
    let main = slot_path(dir, id);
    if main.exists() {
        fs::rename(&main, dir.join(format!("{id}.{EXTENSION}.corrupt")))?;
    }
    fs::copy(backup_path(dir, id), &main)?;
    Ok(save)
}

fn read_meta(path: &Path) -> Result<SaveMeta, SaveError> {
    let text = fs::read_to_string(path)?;
    let header: SaveHeader = ron::from_str(&text).map_err(|e| SaveError::Corrupt(e.to_string()))?;
    check_versions(header.version, &header.meta)?;
    Ok(header.meta)
}

/// Emplacements présents, du plus récemment sauvegardé au plus ancien
/// (les illisibles à la fin, pour pouvoir les restaurer ou les supprimer).
pub fn list_slots(dir: &Path) -> Vec<SlotInfo> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut ids: BTreeSet<String> = BTreeSet::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let id = name
            .strip_suffix(&format!(".{EXTENSION}.bak"))
            .or_else(|| name.strip_suffix(&format!(".{EXTENSION}")));
        if let Some(id) = id {
            ids.insert(id.to_string());
        }
    }
    let mut slots: Vec<SlotInfo> = ids
        .into_iter()
        .map(|id| {
            let main = slot_path(dir, &id);
            let meta = if main.exists() {
                read_meta(&main)
            } else {
                Err(SaveError::Io("sauvegarde absente".into()))
            };
            SlotInfo {
                has_backup: backup_path(dir, &id).exists(),
                id,
                meta,
            }
        })
        .collect();
    slots.sort_by_key(|slot| match &slot.meta {
        Ok(meta) => (0, u64::MAX - meta.saved_unix),
        Err(_) => (1, 0),
    });
    slots
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::placement::PlacedBuildings;
    use crate::inventory::ItemKind;
    use crate::player::INVENTORY_SLOTS;
    use crate::world::deposits::{DepositConfig, DepositField};
    use crate::world::generation::{HeightField, WorldGenConfig};
    use crate::world::rocks::{RockConfig, RockField};

    /// Dossier temporaire propre à un test.
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ascension-test-{name}-{}-{}",
            std::process::id(),
            now_unix()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// Une partie avec la scène de démo, qui a tourné `ticks` ticks.
    fn running_game(ticks: u64) -> SaveGame {
        let seed = WorldSeed(42);
        let field = HeightField::generate(seed, &WorldGenConfig::default());
        let rocks = RockField::generate(seed, &field, &RockConfig::default());
        let deposits = DepositField::generate(seed, &field, &rocks, &DepositConfig::default());
        let mut sim = SimWorld::default();
        let mut placed = PlacedBuildings::default();
        crate::scene::build_demo(&field, &deposits, &rocks, &mut sim, &mut placed).unwrap();
        for _ in 0..ticks {
            sim.step();
        }
        let mut inventory = Inventory::new(INVENTORY_SLOTS);
        inventory.add(ItemKind::IronPlate, 37);
        SaveGame {
            version: SAVE_VERSION,
            meta: SaveMeta {
                name: "Île d'essai".into(),
                seed,
                worldgen_version: WORLDGEN_VERSION.0,
                creative: false,
                created_unix: 1,
                saved_unix: 2,
                ticks: sim.clock().tick(),
            },
            player: PlayerSave {
                position: [1.25, 20.5, -3.0],
                yaw: 0.7,
                pitch: -0.2,
                flying: true,
                inventory,
            },
            sim,
            placed,
            destroyed_rocks: [RockId(3), RockId(9)].into(),
        }
    }

    #[test]
    fn a_reloaded_game_continues_exactly_like_the_original() {
        let mut original = running_game(3_000);
        let mut reloaded = from_ron(&to_ron(&original).unwrap()).unwrap();
        assert_eq!(reloaded.player, original.player);
        assert_eq!(reloaded.destroyed_rocks, original.destroyed_rocks);
        for _ in 0..3_000 {
            original.sim.step();
            reloaded.sim.step();
        }
        // Même état après 3 000 ticks de plus (comparé via le texte sauvegardé).
        assert_eq!(to_ron(&reloaded).unwrap(), to_ron(&original).unwrap());
        assert!(
            original.sim.deliveries().grand_total() > 0,
            "la démo produit"
        );
    }

    #[test]
    fn power_networks_are_rebuilt_on_load() {
        let mut original = running_game(10);
        let mut reloaded = from_ron(&to_ron(&original).unwrap()).unwrap();
        // Les bilans (W produits / demandés) se recalculent au tick suivant.
        original.sim.step();
        reloaded.sim.step();
        assert_eq!(reloaded.sim.grid(), original.sim.grid());
    }

    #[test]
    fn versions_are_checked() {
        let mut save = running_game(0);
        save.version = SAVE_VERSION + 1;
        assert_eq!(
            from_ron(&to_ron(&save).unwrap()).unwrap_err(),
            SaveError::TooNew(SAVE_VERSION + 1)
        );
        save.version = SAVE_VERSION;
        save.meta.worldgen_version = WORLDGEN_VERSION.0 + 1;
        assert!(matches!(
            from_ron(&to_ron(&save).unwrap()),
            Err(SaveError::WorldGenMismatch(_))
        ));
    }

    #[test]
    fn garbage_is_reported_as_corrupt_not_a_crash() {
        assert!(matches!(from_ron(""), Err(SaveError::Corrupt(_))));
        let text = to_ron(&running_game(0)).unwrap();
        let truncated = &text[..text.len() / 2];
        assert!(matches!(from_ron(truncated), Err(SaveError::Corrupt(_))));
    }

    #[test]
    fn writing_keeps_the_previous_save_as_backup() {
        let dir = temp_dir("backup");
        let mut save = running_game(0);
        write_slot(&dir, "ile", &save).unwrap();
        assert!(!backup_path(&dir, "ile").exists());
        save.meta.saved_unix = 99;
        write_slot(&dir, "ile", &save).unwrap();
        assert_eq!(read_slot(&dir, "ile").unwrap().meta.saved_unix, 99);
        assert_eq!(read_backup(&dir, "ile").unwrap().meta.saved_unix, 2);
        assert!(!dir.join("ile.ron.tmp").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_corrupt_slot_is_listed_with_its_backup() {
        let dir = temp_dir("corrupt");
        let mut save = running_game(0);
        write_slot(&dir, "a", &save).unwrap();
        write_slot(&dir, "a", &save).unwrap();
        fs::write(slot_path(&dir, "a"), "(version: 1, meta: (").unwrap();
        save.meta.saved_unix = 50;
        write_slot(&dir, "b", &save).unwrap();
        let slots = list_slots(&dir);
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0].id, "b", "valide et récente d'abord");
        assert!(slots[1].meta.is_err());
        assert!(slots[1].has_backup);
        assert!(read_backup(&dir, "a").is_ok());
        restore_backup(&dir, "a").unwrap();
        assert!(list_slots(&dir).iter().all(|slot| slot.meta.is_ok()));
        assert!(
            dir.join("a.ron.corrupt").exists(),
            "le fichier abîmé est gardé"
        );
        delete_slot(&dir, "a").unwrap();
        assert_eq!(list_slots(&dir).len(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn names_become_unique_file_ids() {
        let dir = temp_dir("names");
        assert_eq!(slug("Île d'Été #2"), "ile-d-ete-2");
        assert_eq!(slug("???"), "monde");
        assert_eq!(clean_name("  Mon   monde "), Some("Mon monde".into()));
        assert_eq!(clean_name("   "), None);
        assert_eq!(clean_name(&"x".repeat(MAX_NAME_CHARS + 1)), None);
        assert_eq!(new_slot_id(&dir, "Base"), "base");
        write_slot(&dir, "base", &running_game(0)).unwrap();
        assert_eq!(new_slot_id(&dir, "Base"), "base-2");
        fs::remove_dir_all(&dir).unwrap();
    }
}
