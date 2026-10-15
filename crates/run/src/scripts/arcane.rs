//! Arcane Mage.
//!
//! [`arcane`] ports SimC's 12.1 Spellslinger and Sunfury priority lists
//! (`profiles/MID2/MID2_Mage_Arcane.simc`).

use portunus_core::{AuraId, HeroTreeId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, Wait};
use portunus_env::Decision;
use portunus_gamedata::stats::ResourceKind;

use crate::observe::SeatObs;

const ARCANE_BLAST: SpellId = SpellId(30451);
const ARCANE_BARRAGE: SpellId = SpellId(44425);
const ARCANE_MISSILES: SpellId = SpellId(5143);
const ARCANE_PULSE: SpellId = SpellId(1241462);
const ARCANE_ORB: SpellId = SpellId(153626);
const TOUCH_OF_THE_MAGI: SpellId = SpellId(321507);
const ARCANE_SURGE: SpellId = SpellId(365350);
const EVOCATION: SpellId = SpellId(12051);
const PRESENCE_OF_MIND: SpellId = SpellId(205025);
const PRISMATIC_BOLT: SpellId = SpellId(1295924);
const SHIFTING_POWER: SpellId = SpellId(314791);

const CLEARCASTING_BUFF: AuraId = AuraId(263725);
const ARCANE_SALVO_BUFF: AuraId = AuraId(1242974);
const ARCANE_SURGE_BUFF: AuraId = AuraId(365362);
const ARCANE_SOUL_BUFF: AuraId = AuraId(451038);
const PRISMATIC_BOLT_BUFF: AuraId = AuraId(1295925);

const TOUCH_OF_THE_MAGI_DEBUFF: AuraId = AuraId(210824);

struct Arc<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
    mana: f64,
    charges: f64,
    salvo: u8,
    cc: u8,
    spellslinger: bool,
}

impl<'a> Arc<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let enemies = obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len());
        let mana = obs.resource(ResourceKind::Mana);
        let charges = obs.resource(ResourceKind::ArcaneCharges);
        let spellslinger = obs.hero_tree == Some(HeroTreeId(40));
        Self {
            obs,
            legal,
            enemies,
            mana,
            charges,
            salvo: obs.buff_stacks(ARCANE_SALVO_BUFF),
            cc: obs.buff_stacks(CLEARCASTING_BUFF),
            spellslinger,
        }
    }

    fn pick(&self, s: SpellId, when: bool) -> Option<SpellId> {
        (when && self.legal.is_ready(s)).then_some(s)
    }

    /// SimC's `cooldowns` list.
    fn cooldowns(&self) -> Option<SpellId> {
        let surge_up = self.obs.has_buff(ARCANE_SURGE_BUFF);
        let totm_up = self.obs.target_aura(TOUCH_OF_THE_MAGI_DEBUFF).is_some();
        [
            self.pick(ARCANE_ORB, self.spellslinger && self.charges < 2.0),
            self.pick(TOUCH_OF_THE_MAGI, surge_up || self.salvo >= 8),
            self.pick(ARCANE_SURGE, true),
            self.pick(EVOCATION, self.mana < 15.0 && !surge_up && !totm_up),
            self.pick(
                PRESENCE_OF_MIND,
                self.charges < 1.0 && !self.obs.has_buff(PRISMATIC_BOLT_BUFF),
            ),
            self.pick(
                SHIFTING_POWER,
                !surge_up
                    && !totm_up
                    && self.obs.cooldown_remaining(ARCANE_SURGE) > SimDuration(15_000),
            ),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `spellslinger` list.
    fn spellslinger_rot(&self) -> Option<SpellId> {
        [
            self.pick(PRISMATIC_BOLT, true),
            self.pick(
                ARCANE_ORB,
                self.obs.charges(ARCANE_ORB) >= 2 && self.enemies >= 2,
            ),
            self.pick(
                ARCANE_BARRAGE,
                self.salvo >= 15 || (self.charges >= 4.0 && self.salvo >= 10),
            ),
            self.pick(ARCANE_MISSILES, self.cc > 0 && self.salvo < 18),
            self.pick(ARCANE_ORB, self.charges < 3.0),
            self.pick(
                ARCANE_PULSE,
                self.charges < 3.0 && !self.obs.has_buff(ARCANE_SURGE_BUFF),
            ),
            self.pick(ARCANE_BLAST, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `sunfury` list.
    fn sunfury_rot(&self) -> Option<SpellId> {
        let soul_up = self.obs.has_buff(ARCANE_SOUL_BUFF);
        [
            self.pick(ARCANE_MISSILES, self.cc > 0 && self.salvo < 12),
            self.pick(PRISMATIC_BOLT, !soul_up),
            self.pick(
                ARCANE_BARRAGE,
                soul_up || (self.charges >= 4.0 && self.salvo >= 12),
            ),
            self.pick(ARCANE_ORB, self.charges < 3.0),
            self.pick(ARCANE_PULSE, self.enemies >= 2 || self.charges < 1.0),
            self.pick(ARCANE_BLAST, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Arcane Mage priority list (Spellslinger & Sunfury).
pub fn arcane(d: &Decision<SeatObs>) -> Choice {
    let a = Arc::new(d);
    let rot = if a.spellslinger {
        a.spellslinger_rot()
    } else {
        a.sunfury_rot()
    };
    a.cooldowns()
        .or(rot)
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
