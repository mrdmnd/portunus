//! Fire Mage.
//!
//! [`fire`] ports SimC's 12.1 Sunfury and Frostfire priority lists
//! (`profiles/MID2/MID2_Mage_Fire.simc`).

use portunus_core::{AuraId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, Wait};
use portunus_env::Decision;

use crate::observe::SeatObs;

const FIREBALL: SpellId = SpellId(133);
const FROSTFIRE_BOLT: SpellId = SpellId(431044);
const FIRE_BLAST: SpellId = SpellId(108853);
const PYROBLAST: SpellId = SpellId(11366);
const FLAMESTRIKE: SpellId = SpellId(2120);
const SCORCH: SpellId = SpellId(2948);
const METEOR: SpellId = SpellId(153561);
const COMBUSTION: SpellId = SpellId(190319);
const SHIFTING_POWER: SpellId = SpellId(314791);

const HEATING_UP_BUFF: AuraId = AuraId(48107);
const HOT_STREAK_BUFF: AuraId = AuraId(48108);
const COMBUSTION_BUFF: AuraId = AuraId(190319);
const HYPERTHERMIA_BUFF: AuraId = AuraId(383874);
const PYROCLASM_BUFF: AuraId = AuraId(269651);
const FROSTFIRE_EMPOWERMENT_BUFF: AuraId = AuraId(431177);

struct Fire<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
    hot_streak: bool,
    heating_up: bool,
    comb_up: bool,
    hyper_up: bool,
    pyroclasm: u8,
    target_hp: f64,
}

impl<'a> Fire<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let enemies = obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len());
        let target_hp = obs.target.as_ref().map_or(1.0, |t| t.health_pct);
        Self {
            obs,
            legal,
            enemies,
            hot_streak: obs.has_buff(HOT_STREAK_BUFF),
            heating_up: obs.has_buff(HEATING_UP_BUFF),
            comb_up: obs.has_buff(COMBUSTION_BUFF),
            hyper_up: obs.has_buff(HYPERTHERMIA_BUFF),
            pyroclasm: obs.buff_stacks(PYROCLASM_BUFF),
            target_hp,
        }
    }

    fn pick(&self, s: SpellId, when: bool) -> Option<SpellId> {
        (when && self.legal.is_ready(s)).then_some(s)
    }

    fn filler_bolt(&self, when: bool) -> Option<SpellId> {
        self.pick(FROSTFIRE_BOLT, when)
            .or_else(|| self.pick(FIREBALL, when))
    }

    fn spender(&self, when: bool) -> Option<SpellId> {
        if self.enemies >= 3 {
            self.pick(FLAMESTRIKE, when)
                .or_else(|| self.pick(PYROBLAST, when))
        } else {
            self.pick(PYROBLAST, when)
        }
    }

    fn rotation(&self) -> Option<SpellId> {
        let fb_charges = self.obs.charges(FIRE_BLAST);
        [
            self.pick(COMBUSTION, !self.comb_up),
            self.pick(
                FIRE_BLAST,
                !self.hot_streak
                    && (self.heating_up || fb_charges >= 3 || (self.comb_up && fb_charges >= 1)),
            ),
            self.pick(METEOR, true),
            self.spender(self.hot_streak || self.hyper_up),
            self.spender(self.pyroclasm >= 2 || (self.pyroclasm >= 1 && self.comb_up)),
            self.filler_bolt(self.obs.has_buff(FROSTFIRE_EMPOWERMENT_BUFF)),
            self.pick(SCORCH, self.target_hp <= 0.3 || self.comb_up),
            self.pick(
                SHIFTING_POWER,
                !self.comb_up
                    && !self.hyper_up
                    && self.obs.cooldown_remaining(COMBUSTION) > SimDuration(15_000),
            ),
            self.filler_bolt(true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Fire Mage priority list (Sunfury & Frostfire).
pub fn fire(d: &Decision<SeatObs>) -> Choice {
    Fire::new(d)
        .rotation()
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
