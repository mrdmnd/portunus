//! Marksmanship Hunter.
//!
//! [`marksmanship`] ports SimC's 12.1 Sentinel and Dark Ranger priority
//! lists (`profiles/MID2/MID2_Hunter_Marksmanship.simc`).

use portunus_core::{AuraId, HeroTreeId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, Wait};
use portunus_env::Decision;
use portunus_gamedata::stats::ResourceKind;

use super::GCD;
use crate::observe::SeatObs;

const AIMED_SHOT: SpellId = SpellId(19434);
const RAPID_FIRE: SpellId = SpellId(257044);
const ARCANE_SHOT: SpellId = SpellId(185358);
const MULTI_SHOT: SpellId = SpellId(257620);
const STEADY_SHOT: SpellId = SpellId(56641);
const EXPLOSIVE_SHOT: SpellId = SpellId(212431);
const KILL_SHOT: SpellId = SpellId(53351);
const KILL_SHOT_DB: SpellId = SpellId(320976);
const TRUESHOT: SpellId = SpellId(288613);
const VOLLEY: SpellId = SpellId(260243);
const MOONLIGHT_CHAKRAM: SpellId = SpellId(1264949);
const BLACK_ARROW: SpellId = SpellId(466930);
const WAILING_ARROW: SpellId = SpellId(392060);

const PRECISE_SHOTS_BUFF: AuraId = AuraId(260242);
const LOCK_AND_LOAD_BUFF: AuraId = AuraId(194594);
const TRICK_SHOTS_BUFF: AuraId = AuraId(257622);
const TRUESHOT_BUFF: AuraId = AuraId(288613);
const BULLETSTORM_BUFF: AuraId = AuraId(389020);
const MOONLIGHT_CHAKRAM_BUFF: AuraId = AuraId(1264946);
const DEATHBLOW_BUFF: AuraId = AuraId(378770);

const SENTINELS_MARK_DEBUFF: AuraId = AuraId(1253601);

const TRICK_SHOTS_TALENT: AuraId = AuraId(257621);
const ASPECT_OF_THE_HYDRA_TALENT: AuraId = AuraId(470945);
const TACTICAL_RELOAD_TALENT: AuraId = AuraId(1301406);

struct Mm<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
    focus: f64,
    dark_ranger: bool,
    ps_up: bool,
    lnl_up: bool,
    ts_rem: SimDuration,
}

impl<'a> Mm<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let enemies = obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len());
        let focus = obs.resource(ResourceKind::Focus);
        let dark_ranger = obs.hero_tree == Some(HeroTreeId(44));
        Self {
            obs,
            legal,
            enemies,
            focus,
            dark_ranger,
            ps_up: obs.has_buff(PRECISE_SHOTS_BUFF),
            lnl_up: obs.has_buff(LOCK_AND_LOAD_BUFF),
            ts_rem: obs.buff_remaining(TRICK_SHOTS_BUFF),
        }
    }

    fn pick(&self, s: SpellId, when: bool) -> Option<SpellId> {
        (when && self.legal.is_ready(s)).then_some(s)
    }

    fn kill_shot(&self, when: bool) -> Option<SpellId> {
        self.pick(KILL_SHOT_DB, when)
            .or_else(|| self.pick(KILL_SHOT, when))
    }

    /// SimC's `sentst` list (Sentinel single-target).
    fn sentst(&self) -> Option<SpellId> {
        let explo_ok = !self.obs.has_buff(TACTICAL_RELOAD_TALENT) || !self.lnl_up;
        let mc_rem = self.obs.buff_remaining(MOONLIGHT_CHAKRAM_BUFF);
        let hydra = self.obs.has_buff(ASPECT_OF_THE_HYDRA_TALENT);
        [
            self.pick(EXPLOSIVE_SHOT, explo_ok),
            self.pick(VOLLEY, true),
            self.pick(TRUESHOT, true),
            self.pick(
                MOONLIGHT_CHAKRAM,
                mc_rem > SimDuration::ZERO && mc_rem < SimDuration(5_000),
            ),
            self.pick(
                AIMED_SHOT,
                self.obs.target_aura(SENTINELS_MARK_DEBUFF).is_some()
                    && self.obs.has_buff(BULLETSTORM_BUFF)
                    && self.enemies > 1,
            ),
            self.pick(RAPID_FIRE, true),
            self.kill_shot(self.ps_up),
            self.pick(MULTI_SHOT, self.ps_up && self.enemies > 1 && hydra),
            self.pick(ARCANE_SHOT, self.ps_up),
            self.pick(AIMED_SHOT, true),
            self.pick(MOONLIGHT_CHAKRAM, true),
            self.pick(STEADY_SHOT, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `sentaoe` list (Sentinel multi-target).
    fn sentaoe(&self) -> Option<SpellId> {
        let explo_ok = !self.obs.has_buff(TACTICAL_RELOAD_TALENT) || !self.lnl_up;
        let mc_rem = self.obs.buff_remaining(MOONLIGHT_CHAKRAM_BUFF);
        let hydra = self.obs.has_buff(ASPECT_OF_THE_HYDRA_TALENT);
        [
            self.pick(EXPLOSIVE_SHOT, explo_ok),
            self.pick(VOLLEY, true),
            self.pick(TRUESHOT, true),
            self.pick(
                MOONLIGHT_CHAKRAM,
                mc_rem > SimDuration::ZERO && mc_rem < GCD,
            ),
            self.pick(
                MULTI_SHOT,
                (self.ps_up && !hydra) || self.ts_rem == SimDuration::ZERO,
            ),
            self.pick(
                AIMED_SHOT,
                self.obs.target_aura(SENTINELS_MARK_DEBUFF).is_some()
                    && self.obs.has_buff(BULLETSTORM_BUFF),
            ),
            self.pick(RAPID_FIRE, true),
            self.pick(AIMED_SHOT, self.ts_rem > SimDuration(2_000)),
            self.pick(MOONLIGHT_CHAKRAM, true),
            self.pick(MULTI_SHOT, self.focus > 65.0),
            self.pick(STEADY_SHOT, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `drst` list (Dark Ranger single-target).
    fn drst(&self) -> Option<SpellId> {
        let explo_ok = !self.obs.has_buff(TACTICAL_RELOAD_TALENT) || !self.lnl_up;
        let hydra = self.obs.has_buff(ASPECT_OF_THE_HYDRA_TALENT);
        let target_exec = self
            .obs
            .target
            .as_ref()
            .is_some_and(|t| t.health_pct <= 0.2 || t.health_pct >= 0.8);
        let ba_ok = target_exec || self.obs.has_buff(DEATHBLOW_BUFF);
        [
            self.pick(BLACK_ARROW, self.ps_up && ba_ok),
            self.pick(EXPLOSIVE_SHOT, explo_ok),
            self.pick(VOLLEY, true),
            self.pick(TRUESHOT, true),
            self.pick(WAILING_ARROW, true),
            self.pick(
                AIMED_SHOT,
                (self.obs.has_buff(TRUESHOT_BUFF) && !self.ps_up)
                    || self.obs.charges(AIMED_SHOT) >= 2,
            ),
            self.pick(RAPID_FIRE, true),
            self.pick(MULTI_SHOT, self.ps_up && self.enemies > 1 && hydra),
            self.pick(ARCANE_SHOT, self.ps_up),
            self.pick(AIMED_SHOT, true),
            self.pick(BLACK_ARROW, ba_ok),
            self.pick(STEADY_SHOT, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `draoe` list (Dark Ranger multi-target).
    fn draoe(&self) -> Option<SpellId> {
        let explo_ok = !self.obs.has_buff(TACTICAL_RELOAD_TALENT) || !self.lnl_up;
        [
            self.pick(EXPLOSIVE_SHOT, explo_ok),
            self.pick(VOLLEY, true),
            self.pick(
                AIMED_SHOT,
                self.ts_rem > SimDuration(2_000) && self.obs.charges(AIMED_SHOT) >= 2,
            ),
            self.pick(BLACK_ARROW, self.ps_up),
            self.pick(MULTI_SHOT, self.ts_rem == SimDuration::ZERO),
            self.pick(TRUESHOT, true),
            self.pick(RAPID_FIRE, self.ts_rem > SimDuration(1_500)),
            self.pick(WAILING_ARROW, !self.legal.is_ready(BLACK_ARROW)),
            self.pick(MULTI_SHOT, self.ps_up),
            self.pick(AIMED_SHOT, self.ts_rem > SimDuration(2_000)),
            self.pick(BLACK_ARROW, true),
            self.pick(MULTI_SHOT, self.focus > 65.0),
            self.pick(STEADY_SHOT, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Marksmanship Hunter priority list (Sentinel & Dark Ranger).
pub fn marksmanship(d: &Decision<SeatObs>) -> Choice {
    let mm = Mm::new(d);
    let aoe = mm.enemies > 2 && mm.obs.has_buff(TRICK_SHOTS_TALENT);
    let rot = match (mm.dark_ranger, aoe) {
        (true, true) => mm.draoe(),
        (true, false) => mm.drst(),
        (false, true) => mm.sentaoe(),
        (false, false) => mm.sentst(),
    };
    rot.map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
