//! Bestiary loader: enemy definitions from TOML into codex data.
//!
//! Enemies are codex citizens — calibrated data versioned with the game
//! build. This loader lives here only until a real `sim-codex` crate exists;
//! the output type ([`Bestiary`]) already lives in `sim-types::codex`.

use std::collections::BTreeMap;

use serde::Deserialize;
use sim_types::{
    Bestiary, EnemyDef, EnemyScriptDef, HpSpec, IntakeTargetSpec, JitteredMs, ScriptEffect,
};
use thiserror::Error;

/// Bestiary load errors.
#[derive(Debug, Error)]
pub enum BestiaryError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(#[from] toml::de::Error),
}

#[derive(Debug, Deserialize)]
struct BestiaryToml {
    /// BTreeMap keeps enemy order deterministic regardless of file order.
    enemies: BTreeMap<String, EnemyToml>,
}

#[derive(Debug, Deserialize)]
struct EnemyToml {
    hp: HpSpec,
    #[serde(default)]
    count: u32,
    #[serde(default)]
    script: Vec<ScriptToml>,
}

#[derive(Debug, Deserialize)]
struct ScriptToml {
    id: String,
    first_ms: JitteredMs,
    every_ms: Option<JitteredMs>,
    #[serde(flatten)]
    effect: EffectToml,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum EffectToml {
    Intake { amount: f64, target: TargetToml },
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum TargetToml {
    AllPlayers,
    RandomPlayer,
}

impl From<TargetToml> for IntakeTargetSpec {
    fn from(t: TargetToml) -> Self {
        match t {
            TargetToml::AllPlayers => IntakeTargetSpec::AllPlayers,
            TargetToml::RandomPlayer => IntakeTargetSpec::RandomPlayer,
        }
    }
}

/// Load a bestiary from TOML source.
pub fn load_str(src: &str) -> Result<Bestiary, BestiaryError> {
    let raw: BestiaryToml = toml::from_str(src)?;
    let enemies = raw
        .enemies
        .into_iter()
        .map(|(name, e)| EnemyDef {
            name,
            hp: e.hp,
            count: e.count,
            script: e
                .script
                .into_iter()
                .map(|s| EnemyScriptDef {
                    id: s.id,
                    first: s.first_ms,
                    every: s.every_ms,
                    effect: match s.effect {
                        EffectToml::Intake { amount, target } => {
                            ScriptEffect::Intake { amount, target: target.into() }
                        }
                    },
                })
                .collect(),
        })
        .collect();
    Ok(Bestiary { enemies })
}

/// Load a bestiary from a TOML file.
pub fn load_path(path: &std::path::Path) -> Result<Bestiary, BestiaryError> {
    load_str(&std::fs::read_to_string(path)?)
}
