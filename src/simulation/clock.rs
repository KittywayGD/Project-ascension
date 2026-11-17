//! Horloge de simulation à pas fixe.
//!
//! Volontairement indépendante de Bevy (voir TODO §1 : simulation en Rust pur) :
//! le temps simulé se compte en ticks entiers, jamais en secondes flottantes.

use serde::{Deserialize, Serialize};

/// Fréquence du tick de simulation. Valeur provisoire, à mesurer (TODO M3).
pub const TICK_RATE_HZ: u32 = 20;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimClock {
    tick: u64,
}

impl SimClock {
    /// Avance la simulation d'un tick.
    pub fn advance(&mut self) {
        self.tick += 1;
    }

    /// Nombre de ticks simulés depuis le début de la partie.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Temps simulé total, en millisecondes (calcul entier, sans dérive).
    pub fn elapsed_millis(&self) -> u64 {
        self.tick * 1000 / u64::from(TICK_RATE_HZ)
    }

    /// Temps simulé au format `hh:mm:ss.cc`.
    pub fn format_elapsed(&self) -> String {
        let ms = self.elapsed_millis();
        let (h, m, s, cs) = (
            ms / 3_600_000,
            ms / 60_000 % 60,
            ms / 1000 % 60,
            ms / 10 % 100,
        );
        format!("{h:02}:{m:02}:{s:02}.{cs:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock_at(ticks: u64) -> SimClock {
        let mut clock = SimClock::default();
        for _ in 0..ticks {
            clock.advance();
        }
        clock
    }

    #[test]
    fn starts_at_zero() {
        let clock = SimClock::default();
        assert_eq!(clock.tick(), 0);
        assert_eq!(clock.format_elapsed(), "00:00:00.00");
    }

    #[test]
    fn one_second_is_tick_rate_ticks() {
        let clock = clock_at(u64::from(TICK_RATE_HZ));
        assert_eq!(clock.elapsed_millis(), 1000);
    }

    #[test]
    fn long_runs_do_not_drift() {
        // 10 heures simulées : le calcul entier tombe pile.
        let ticks = 10 * 3600 * u64::from(TICK_RATE_HZ);
        let clock = SimClock { tick: ticks };
        assert_eq!(clock.elapsed_millis(), 36_000_000);
        assert_eq!(clock.format_elapsed(), "10:00:00.00");
    }

    #[test]
    fn formats_minutes_and_centiseconds() {
        // 83,25 s → 1 min 23,25 s
        let clock = SimClock {
            tick: 8325 * u64::from(TICK_RATE_HZ) / 100,
        };
        assert_eq!(clock.format_elapsed(), "00:01:23.25");
    }
}
