//! Devourer Demon Hunter.
//!
//! [`devourer`] ports SimC's 12.1 Annihilator and Void-Scarred priority
//! lists (`profiles/MID2/MID2_Demon_Hunter_Devourer.simc`).

use portunus_core::{AuraId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, Wait};
use portunus_env::Decision;

use crate::observe::SeatObs;

const CONSUME: SpellId = SpellId(473662);
const DEVOUR: SpellId = SpellId(1217605);
const REAP: SpellId = SpellId(1226019);
const CULL: SpellId = SpellId(1226025);
const VOIDBLADE: SpellId = SpellId(1215602);
const VOID_RAY: SpellId = SpellId(473728);
const SOUL_IMMOLATION: SpellId = SpellId(1241937);
const COLLAPSING_STAR: SpellId = SpellId(1221167);
const HUNGERING_SLASH: SpellId = SpellId(1239519);
const THE_HUNT: SpellId = SpellId(1246167);
const ERADICATE: SpellId = SpellId(1226033);
const METAMORPHOSIS: SpellId = SpellId(191427);

const SOUL_FRAGMENTS_BUFF: AuraId = AuraId(1226020);
const SOUL_IMMOLATION_DOT: AuraId = AuraId(1241937);

struct Dev<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    fragments: u8,
}

impl<'a> Dev<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        Self {
            obs,
            legal,
            fragments: obs.buff_stacks(SOUL_FRAGMENTS_BUFF),
        }
    }

    fn pick(&self, s: SpellId, when: bool) -> Option<SpellId> {
        (when && self.legal.is_ready(s)).then_some(s)
    }

    fn reap(&self, when: bool) -> Option<SpellId> {
        self.pick(ERADICATE, when)
            .or_else(|| self.pick(CULL, when))
            .or_else(|| self.pick(REAP, when))
    }

    fn filler(&self, when: bool) -> Option<SpellId> {
        self.pick(DEVOUR, when).or_else(|| self.pick(CONSUME, when))
    }

    fn rotation(&self) -> Option<SpellId> {
        let imm_refresh = self
            .obs
            .target_aura(SOUL_IMMOLATION_DOT)
            .is_none_or(|a| a.remaining.is_some_and(|r| r <= SimDuration(3600)));
        [
            self.pick(METAMORPHOSIS, true),
            self.pick(THE_HUNT, true),
            self.pick(COLLAPSING_STAR, true),
            self.pick(ERADICATE, true),
            self.pick(HUNGERING_SLASH, true),
            self.pick(VOIDBLADE, true),
            self.reap(self.fragments >= 4),
            self.pick(VOID_RAY, true),
            self.pick(SOUL_IMMOLATION, imm_refresh),
            self.filler(true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Devourer Demon Hunter priority list (Annihilator & Void-Scarred).
pub fn devourer(d: &Decision<SeatObs>) -> Choice {
    Dev::new(d)
        .rotation()
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
