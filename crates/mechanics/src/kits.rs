//! Spec kits and the registry that holds them.

use std::collections::{BTreeMap, BTreeSet};

use portunus_core::{HookKey, Seat, SpecId, SpellId};
use portunus_engine::mechanics::TimerEvent;
use portunus_engine::{EngineIo, Readiness, StateView};
use portunus_gamedata::effect::Effect;
use portunus_gamedata::spell::CastKind;
use portunus_gamedata::GameData;

use crate::{EffectCtx, KitIssue, KitTools, SpecKit, SpecRegistry};

/// Every kit, by spec.
pub struct Kits {
    kits: BTreeMap<SpecId, Box<dyn SpecKit>>,
}

impl Kits {
    /// A later kit for the same spec replaces an earlier one.
    pub fn new(kits: Vec<Box<dyn SpecKit>>) -> Self {
        Self {
            kits: kits.into_iter().map(|k| (k.spec(), k)).collect(),
        }
    }

    /// Every kit this crate implements.
    pub fn standard() -> Self {
        Self::new(vec![Box::new(Elemental)])
    }
}

impl SpecRegistry for Kits {
    fn kit(&self, spec: SpecId) -> Option<&dyn SpecKit> {
        self.kits.get(&spec).map(|k| &**k)
    }

    fn hook_owner(&self, key: &HookKey) -> Option<&dyn SpecKit> {
        self.kits
            .values()
            .find(|k| k.hooks().contains(key))
            .map(|k| &**k)
    }

    fn validate(&self, data: &GameData) -> Vec<KitIssue> {
        let mut issues: Vec<KitIssue> = data
            .specs
            .keys()
            .filter(|s| !self.kits.contains_key(s))
            .map(|&s| KitIssue::MissingKit(s))
            .collect();
        let mut owned = BTreeSet::new();
        for kit in self.kits.values() {
            for key in kit.hooks() {
                if !owned.insert(key) {
                    issues.push(KitIssue::DuplicateHook(key.clone()));
                }
            }
        }
        issues.extend(
            hooks_in(data)
                .into_iter()
                .filter(|k| !owned.contains(k))
                .map(KitIssue::UnknownHook),
        );
        issues
    }
}

/// Every hook named anywhere in the data.
fn hooks_in(data: &GameData) -> BTreeSet<HookKey> {
    fn walk(effects: &[Effect], out: &mut BTreeSet<HookKey>) {
        for e in effects {
            match e {
                Effect::Hook(key) => {
                    out.insert(key.clone());
                }
                Effect::RandomOf(branches) => {
                    for (_, branch) in branches {
                        walk(std::slice::from_ref(branch), out);
                    }
                }
                Effect::If {
                    then, otherwise, ..
                } => {
                    walk(then, out);
                    walk(otherwise, out);
                }
                _ => {}
            }
        }
    }
    let mut out = BTreeSet::new();
    for spell in data.spells.values() {
        walk(&spell.effects, &mut out);
        if let CastKind::Empower { stage_effects, .. } = &spell.cast {
            for stage in stage_effects {
                walk(stage, &mut out);
            }
        }
    }
    for aura in data.auras.values() {
        if let Some(p) = &aura.periodic {
            walk(&p.effects, &mut out);
        }
        if let Some(v) = &aura.value {
            walk(&v.on_threshold, &mut out);
        }
        for l in &aura.listeners {
            walk(&l.effects, &mut out);
        }
        walk(&aura.on_expire, &mut out);
    }
    out
}

/// Elemental Shaman. Everything it does so far is data.
#[derive(Debug, Clone, Copy, Default)]
pub struct Elemental;

impl SpecKit for Elemental {
    fn spec(&self) -> SpecId {
        SpecId(262)
    }

    fn hooks(&self) -> &[HookKey] {
        &[]
    }

    fn run_hook(
        &self,
        _tools: KitTools<'_>,
        _io: &mut dyn EngineIo,
        _ctx: &EffectCtx,
        _key: &HookKey,
    ) {
    }

    fn timer(&self, _tools: KitTools<'_>, _io: &mut dyn EngineIo, _timer: &TimerEvent) {}

    fn gate(&self, _view: &dyn StateView, _seat: Seat, _ability: SpellId) -> Readiness {
        Readiness::Now
    }
}
