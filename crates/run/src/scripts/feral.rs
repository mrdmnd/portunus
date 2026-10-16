//! Feral Druid.
//!
//! [`feral`] ports SimC's 12.1 Druid of the Claw and Wildstalker priority
//! lists (`profiles/MID2/MID2_Druid_Feral.simc`).

use portunus_core::{AuraId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, MoveGoal, Wait};
use portunus_env::Decision;
use portunus_gamedata::spec::MELEE_RANGE;
use portunus_gamedata::stats::ResourceKind;

use crate::observe::SeatObs;

const PROWL: SpellId = SpellId(5215);
const SHRED: SpellId = SpellId(5221);
const RAKE: SpellId = SpellId(1822);
const SWIPE: SpellId = SpellId(106785);
const THRASH: SpellId = SpellId(106830);
const RIP: SpellId = SpellId(1079);
const PRIMAL_WRATH: SpellId = SpellId(285381);
const FEROCIOUS_BITE: SpellId = SpellId(22568);
const RAVAGE: SpellId = SpellId(441591);
const TIGERS_FURY: SpellId = SpellId(5217);
const BERSERK: SpellId = SpellId(106951);
const FERAL_FRENZY: SpellId = SpellId(274837);
const FRANTIC_FRENZY: SpellId = SpellId(1243807);
const CHOMP: SpellId = SpellId(1244258);
const CONVOKE_THE_SPIRITS: SpellId = SpellId(391528);

const PROWL_BUFF: AuraId = AuraId(5215);
const APEX_BUFF: AuraId = AuraId(391882);
const RAVAGE_READY_BUFF: AuraId = AuraId(441585);

const RAKE_DOT: AuraId = AuraId(155722);
const RIP_DOT: AuraId = AuraId(1079);
const THRASH_DOT: AuraId = AuraId(405233);

struct Feral<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
    cp: f64,
}

impl<'a> Feral<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let enemies = obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len());
        let cp = obs.resource(ResourceKind::ComboPoints);
        Self {
            obs,
            legal,
            enemies,
            cp,
        }
    }

    fn pick(&self, s: SpellId, when: bool) -> Option<SpellId> {
        (when && self.legal.is_ready(s)).then_some(s)
    }

    fn dot_refreshable(&self, aura: AuraId, threshold_ms: u32) -> bool {
        self.obs
            .target_aura(aura)
            .is_none_or(|a| a.remaining.is_some_and(|r| r <= SimDuration(threshold_ms)))
    }

    fn bite(&self, when: bool) -> Option<SpellId> {
        self.pick(RAVAGE, when)
            .or_else(|| self.pick(FEROCIOUS_BITE, when))
    }

    fn rotation(&self) -> Option<SpellId> {
        let rake_refresh = self.dot_refreshable(RAKE_DOT, 4500);
        let rip_refresh = self.dot_refreshable(RIP_DOT, 7200);
        let thrash_refresh = self.dot_refreshable(THRASH_DOT, 4500);
        [
            self.pick(TIGERS_FURY, true),
            self.pick(RAKE, self.obs.has_buff(PROWL_BUFF)),
            self.pick(CHOMP, true),
            self.pick(BERSERK, true),
            self.bite(self.obs.has_buff(APEX_BUFF)),
            self.pick(
                PRIMAL_WRATH,
                self.enemies >= 2 && self.cp >= 5.0 && rip_refresh,
            ),
            self.pick(RIP, self.cp >= 5.0 && rip_refresh),
            self.bite(self.cp >= 5.0 || self.obs.has_buff(RAVAGE_READY_BUFF)),
            self.pick(FRANTIC_FRENZY, self.cp <= 2.0),
            self.pick(FERAL_FRENZY, self.cp <= 2.0),
            self.pick(CONVOKE_THE_SPIRITS, self.cp <= 2.0),
            self.pick(PROWL, rake_refresh),
            self.pick(RAKE, rake_refresh),
            self.pick(THRASH, thrash_refresh && self.enemies >= 2),
            self.pick(SWIPE, self.enemies >= 3),
            self.pick(SHRED, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Feral Druid priority list (Druid of the Claw & Wildstalker).
pub fn feral(d: &Decision<SeatObs>) -> Choice {
    if let Some(t) = &d.obs.target {
        if t.distance > MELEE_RANGE && d.obs.movement.is_none() && d.legal.can_move {
            return Choice::Move(MoveGoal::Approach {
                target: t.actor,
                within: MELEE_RANGE,
            });
        }
    }
    Feral::new(d)
        .rotation()
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
