# Sauvegardes (V0.1)

> Code : `src/persistence.rs` (format, disque, en Rust pur) et `src/save.rs` (côté Bevy : F5, autosave, chargement).

## Où

| Système | Dossier |
|---|---|
| macOS | `~/Library/Application Support/Project Ascension/saves/` |
| Linux | `$XDG_DATA_HOME/project-ascension/saves/` (sinon `~/.local/share/…`) |
| Windows | `%APPDATA%\Project Ascension\saves\` |

La variable d'environnement `ASCENSION_SAVE_DIR` remplace ce dossier (tests, essais). Le chemin est rappelé en bas de l'écran titre.

Un monde = un fichier `<emplacement>.ron`, dont le nom vient du nom du monde (`Île d'Été` → `ile-d-ete.ron`, `-2` en cas de doublon).

Les **options** du joueur (souris, affichage, sauvegarde automatique) sont dans `settings.ron`, à côté du dossier `saves/` : elles valent pour tous les mondes.

## Contenu

Texte **RON** lisible, versionné :

```text
(
    version: 1,              // SAVE_VERSION
    meta: (name, seed, worldgen_version, creative, created_unix, saved_unix, ticks),
    player: (position, yaw, pitch, flying, inventory),
    sim: (...),              // SimWorld : bâtiments, convoyeurs, modules, inventaires, recettes, livraisons
    placed: (...),           // bâtiments posés : position, rotation, fondation
    destroyed_rocks: [...],
)
```

- Le **monde n'est pas stocké** : il est régénéré à partir de la seed. Une sauvegarde d'une autre version du générateur (`WORLDGEN_VERSION`) est refusée, car le relief serait différent.
- Les **réseaux électriques** ne sont pas stockés : ils sont recalculés au chargement (`SimWorld::after_load`).
- La **progression de l'arbre techno** se déduit des livraisons du hub : rien à stocker en plus.
- Tout est indexé par des **identifiants stables** (`BuildingId`, `RockId`, `DepositId`), jamais par des entités Bevy.

## Sûreté

- **Écriture atomique** : le fichier est écrit en `.ron.tmp`, puis l'ancienne sauvegarde devient `.ron.bak` et la nouvelle prend sa place. Une coupure pendant l'écriture laisse toujours une copie valide.
- **Sauvegarde illisible** (tronquée, modifiée à la main) : l'écran titre l'affiche avec la raison et propose « Restaurer la copie précédente ». Le fichier abîmé est conservé en `.ron.corrupt`.
- **Version plus récente** du format : refusée avec un message, jamais écrasée.

## Quand

- **F5**, ou « Sauvegarder » dans le menu pause.
- **Automatiquement** toutes les 2 minutes réelles par défaut (sauf en pause) ; réglable dans les options (jamais, 1, 2, 5 ou 10 min).
- En **revenant au menu** et en **quittant** (menu pause, Maj+Échap ou fermeture de la fenêtre).
- À la **création** d'un monde depuis l'écran titre.

Une partie lancée en ligne de commande (`--seed`, `--demo`) sans `--name` n'a pas d'emplacement : elle n'est sauvegardée qu'à la première pression sur F5.

## Jeu fermé

**Aucune production hors ligne** : la simulation reprend exactement au tick sauvegardé.

## Faire évoluer le format

Incrémenter `SAVE_VERSION` à chaque changement incompatible et ajouter une conversion depuis l'ancienne version. Aujourd'hui, une version plus ancienne est refusée (aucune n'existe encore).
