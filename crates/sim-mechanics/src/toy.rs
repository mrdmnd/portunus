//! Toy codex loader (SCAFFOLD.md §0 invariant 2, §12 G0).
//!
//! Loads a synthetic spec ("Trainee") from TOML and compiles it to
//! `(CompiledActor, OpArena)` **through the same effect-IR path as real
//! content** — no `Scripted` escape hatches. This is the kernel-layering
//! test: the engine and mechanics crates must pass their tests with only
//! this data.

use serde::Deserialize;
use sim_types::{
    from_millis, AbilityDef, AuraDef, AuraId, CompiledActor, EffectRange, ResourceCost,
    ResourceDef, ResourceId, StreamId, TickDef,
};
use thiserror::Error;

use crate::ops::{EffectOp, OpArena};

/// Toy-codex load/compile errors.
#[derive(Debug, Error)]
pub enum ToyCodexError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("unknown resource name: {0}")]
    UnknownResource(String),
    #[error("unknown aura name: {0}")]
    UnknownAura(String),
}

// ---- raw TOML schema -------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ToySpec {
    name: String,
    max_health: f64,
    gcd_ms: u64,
    #[serde(default)]
    resources: Vec<ToyResource>,
    #[serde(default)]
    auras: Vec<ToyAura>,
    #[serde(default)]
    abilities: Vec<ToyAbility>,
}

#[derive(Debug, Deserialize)]
struct ToyResource {
    name: String,
    max: f64,
    initial: f64,
    regen_per_sec: f64,
}

#[derive(Debug, Deserialize)]
struct ToyAura {
    name: String,
    duration_ms: u64,
    rate_mult: Option<ToyRateMult>,
    tick: Option<ToyTick>,
}

#[derive(Debug, Deserialize)]
struct ToyRateMult {
    resource: String,
    mult: f64,
}

#[derive(Debug, Deserialize)]
struct ToyTick {
    period_ms: u64,
    effect: Vec<ToyOp>,
}

#[derive(Debug, Deserialize)]
struct ToyAbility {
    name: String,
    cost: Option<ToyCost>,
    #[serde(default)]
    cooldown_ms: u64,
    #[serde(default)]
    cast_time_ms: u64,
    #[serde(default = "default_true")]
    on_gcd: bool,
    #[serde(default = "default_true")]
    targeted: bool,
    #[serde(default)]
    effect: Vec<ToyOp>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct ToyCost {
    resource: String,
    amount: f64,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum ToyOp {
    DealDamage { amount: f64 },
    ApplyAura { aura: String, #[serde(default)] on_self: bool },
    GrantResource { resource: String, amount: f64 },
    SpendResource { resource: String, amount: f64 },
    TriggerProc { chance: f64, stream: u16, effect: Vec<ToyOp> },
}

// ---- compiler ---------------------------------------------------------------

struct Compiler<'s> {
    spec: &'s ToySpec,
    arena: OpArena,
}

impl Compiler<'_> {
    fn resource_id(&self, name: &str) -> Result<ResourceId, ToyCodexError> {
        self.spec
            .resources
            .iter()
            .position(|r| r.name == name)
            .map(|i| ResourceId(i as u8))
            .ok_or_else(|| ToyCodexError::UnknownResource(name.to_string()))
    }

    fn aura_id(&self, name: &str) -> Result<AuraId, ToyCodexError> {
        self.spec
            .auras
            .iter()
            .position(|a| a.name == name)
            .map(|i| AuraId(i as u16))
            .ok_or_else(|| ToyCodexError::UnknownAura(name.to_string()))
    }

    /// Compile an op list into a contiguous arena range. Nested (child)
    /// ranges are emitted before the parent list so all ranges stay
    /// contiguous.
    fn compile_list(&mut self, specs: &[ToyOp]) -> Result<EffectRange, ToyCodexError> {
        let mut resolved: Vec<EffectOp> = Vec::with_capacity(specs.len());
        for s in specs {
            resolved.push(match s {
                ToyOp::DealDamage { amount } => EffectOp::DealDamage { amount: *amount },
                ToyOp::ApplyAura { aura, on_self } => EffectOp::ApplyAura {
                    aura: self.aura_id(aura)?,
                    on_self: *on_self,
                },
                ToyOp::GrantResource { resource, amount } => EffectOp::GrantResource {
                    resource: self.resource_id(resource)?,
                    amount: *amount,
                },
                ToyOp::SpendResource { resource, amount } => EffectOp::SpendResource {
                    resource: self.resource_id(resource)?,
                    amount: *amount,
                },
                ToyOp::TriggerProc { chance, stream, effect } => {
                    let child = self.compile_list(effect)?;
                    EffectOp::TriggerProc {
                        chance: *chance,
                        stream: StreamId(*stream),
                        effect: child,
                    }
                }
            });
        }
        Ok(self.arena.push_list(resolved))
    }
}

/// Compile a toy spec from TOML source.
pub fn compile_str(src: &str) -> Result<(CompiledActor, OpArena), ToyCodexError> {
    let spec: ToySpec = toml::from_str(src)?;
    let mut c = Compiler { spec: &spec, arena: OpArena::default() };

    let mut auras: Vec<AuraDef> = Vec::with_capacity(spec.auras.len());
    for a in &spec.auras {
        let tick = match &a.tick {
            Some(t) => Some(TickDef {
                period: from_millis(t.period_ms),
                effect: c.compile_list(&t.effect)?,
            }),
            None => None,
        };
        let rate_mult = match &a.rate_mult {
            Some(rm) => Some((c.resource_id(&rm.resource)?, rm.mult)),
            None => None,
        };
        auras.push(AuraDef {
            name: a.name.clone(),
            duration: from_millis(a.duration_ms),
            rate_mult,
            tick,
        });
    }

    let mut abilities: Vec<AbilityDef> = Vec::with_capacity(spec.abilities.len());
    for ab in &spec.abilities {
        let cost = match &ab.cost {
            Some(cst) => Some(ResourceCost {
                resource: c.resource_id(&cst.resource)?,
                amount: cst.amount,
            }),
            None => None,
        };
        abilities.push(AbilityDef {
            name: ab.name.clone(),
            cost,
            cooldown: from_millis(ab.cooldown_ms),
            cast_time: from_millis(ab.cast_time_ms),
            on_gcd: ab.on_gcd,
            targeted: ab.targeted,
            effect: c.compile_list(&ab.effect)?,
        });
    }

    let actor = CompiledActor {
        name: spec.name.clone(),
        max_health: spec.max_health,
        gcd: from_millis(spec.gcd_ms),
        resources: spec
            .resources
            .iter()
            .map(|r| ResourceDef {
                name: r.name.clone(),
                max: r.max,
                initial: r.initial,
                regen_per_sec: r.regen_per_sec,
            })
            .collect(),
        abilities,
        auras,
    };
    Ok((actor, c.arena))
}

/// Compile a toy spec from a TOML file.
pub fn compile_path(path: &std::path::Path) -> Result<(CompiledActor, OpArena), ToyCodexError> {
    compile_str(&std::fs::read_to_string(path)?)
}
