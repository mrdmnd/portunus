//! Frost Mage.
//!
//! [`frost_mage`] ports SimC's 12.1 Spellslinger and Frostfire priority lists
//! (`profiles/MID2/MID2_Mage_Frost.simc`).

use portunus_core::{AuraId, SpellId};
use portunus_engine::{ActionMask, Choice, Wait};
use portunus_env::Decision;

use crate::observe::SeatObs;

const FROSTBOLT: SpellId = SpellId(116);
const FROSTFIRE_BOLT: SpellId = SpellId(431044);
const ICE_LANCE: SpellId = SpellId(30455);
const FLURRY: SpellId = SpellId(44614);
const FROZEN_ORB: SpellId = SpellId(84714);
const BLIZZARD: SpellId = SpellId(190356);
const RAY_OF_FROST: SpellId = SpellId(205021);
const COMET_STORM: SpellId = SpellId(1247777);
const GLACIAL_SPIKE: SpellId = SpellId(199786);
const SHIFTING_POWER: SpellId = SpellId(314791);

const FINGERS_OF_FROST_BUFF: AuraId = AuraId(44544);
const BRAIN_FREEZE_BUFF: AuraId = AuraId(190446);
const GLACIAL_SPIKE_READY_BUFF: AuraId = AuraId(199844);
const FREEZING_RAIN_BUFF: AuraId = AuraId(270232);
const THERMAL_VOID_BUFF: AuraId = AuraId(1247730);
const FROSTFIRE_EMPOWERMENT_BUFF: AuraId = AuraId(431177);

const FREEZING_DEBUFF: AuraId = AuraId(1221389);

struct FrostMage<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
    fof: u8,
    bf: bool,
    gs_ready: bool,
    freezing: u8,
}

impl<'a> FrostMage<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let enemies = obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len());
        let freezing = obs.target_aura(FREEZING_DEBUFF).map_or(0, |a| a.stacks);
        Self {
            obs,
            legal,
            enemies,
            fof: obs.buff_stacks(FINGERS_OF_FROST_BUFF),
            bf: obs.has_buff(BRAIN_FREEZE_BUFF),
            gs_ready: obs.has_buff(GLACIAL_SPIKE_READY_BUFF),
            freezing,
        }
    }

    fn pick(&self, s: SpellId, when: bool) -> Option<SpellId> {
        (when && self.legal.is_ready(s)).then_some(s)
    }

    fn filler_bolt(&self, when: bool) -> Option<SpellId> {
        self.pick(FROSTFIRE_BOLT, when)
            .or_else(|| self.pick(FROSTBOLT, when))
    }

    fn rotation(&self) -> Option<SpellId> {
        [
            self.pick(GLACIAL_SPIKE, self.gs_ready),
            self.pick(COMET_STORM, true),
            self.pick(FLURRY, self.bf && !self.obs.has_buff(THERMAL_VOID_BUFF)),
            self.pick(ICE_LANCE, self.fof >= 2),
            self.pick(FROZEN_ORB, true),
            self.pick(
                BLIZZARD,
                self.enemies >= 3 && self.obs.has_buff(FREEZING_RAIN_BUFF),
            ),
            self.pick(ICE_LANCE, self.fof >= 1 || self.freezing >= 6),
            self.pick(RAY_OF_FROST, true),
            self.filler_bolt(self.obs.has_buff(FROSTFIRE_EMPOWERMENT_BUFF)),
            self.pick(FLURRY, self.obs.charges(FLURRY) >= 2),
            self.pick(SHIFTING_POWER, self.fof == 0 && !self.bf),
            self.filler_bolt(true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Frost Mage priority list (Spellslinger & Frostfire).
pub fn frost_mage(d: &Decision<SeatObs>) -> Choice {
    FrostMage::new(d)
        .rotation()
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
