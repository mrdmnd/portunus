//! Beast Mastery Hunter.
//!
//! [`beast_mastery`] ports SimC's 12.1 Pack Leader and Dark Ranger priority
//! lists (`profiles/MID2/MID2_Hunter_Beast_Mastery.simc`).

use portunus_core::{AuraId, HeroTreeId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, Wait};
use portunus_env::Decision;
use portunus_gamedata::stats::ResourceKind;

use super::GCD;
use crate::observe::{SeatObs, FOREVER};

const KILL_COMMAND: SpellId = SpellId(34026);
const BARBED_SHOT: SpellId = SpellId(217200);
const COBRA_SHOT: SpellId = SpellId(193455);
const BESTIAL_WRATH: SpellId = SpellId(19574);
const WILD_THRASH: SpellId = SpellId(1264359);
const BLACK_ARROW: SpellId = SpellId(466930);
const WAILING_ARROW: SpellId = SpellId(392060);

const BESTIAL_WRATH_BUFF: AuraId = AuraId(19574);
const BEAST_CLEAVE_BUFF: AuraId = AuraId(118455);
const NATURES_ALLY_BUFF: AuraId = AuraId(1276720);
const HOWL_READY_BUFF: AuraId = AuraId(471878);
const WITHERING_FIRE_BUFF: AuraId = AuraId(466991);
const DEATHBLOW_BUFF: AuraId = AuraId(378770);
const HOGSTRIDER_BUFF: AuraId = AuraId(472640);

const BARBED_SHOT_DOT: AuraId = AuraId(217200);

const BEAST_CLEAVE_TALENT: AuraId = AuraId(115939);
const KILLER_COBRA_TALENT: AuraId = AuraId(199532);
const SERPENTINE_STRIKES_TALENT: AuraId = AuraId(468701);
const NATURES_ALLY_R4_TALENT: AuraId = AuraId(1273126);

struct Bm<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
    focus: f64,
    dark_ranger: bool,
}

impl<'a> Bm<'a> {
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
        }
    }

    fn known(&self, s: SpellId) -> bool {
        self.legal.abilities.contains_key(&s)
    }

    fn cd(&self, s: SpellId) -> SimDuration {
        if self.known(s) {
            self.obs.cooldown_remaining(s)
        } else {
            FOREVER
        }
    }

    fn pick(&self, s: SpellId, when: bool) -> Option<SpellId> {
        (when && self.legal.is_ready(s)).then_some(s)
    }

    /// SimC's `st` list (Pack Leader single-target).
    fn st(&self) -> Option<SpellId> {
        let bw_rem = self.cd(BESTIAL_WRATH);
        let barbed_dot_rem = self
            .obs
            .target_aura(BARBED_SHOT_DOT)
            .and_then(|a| a.remaining)
            .unwrap_or(SimDuration::ZERO);
        let kc_charges = self.obs.charges(KILL_COMMAND);
        let barbed_charges = self.obs.charges(BARBED_SHOT);
        let apex_r4 = self.obs.has_buff(NATURES_ALLY_R4_TALENT);
        let na_up = self.obs.has_buff(NATURES_ALLY_BUFF);
        [
            self.pick(
                BARBED_SHOT,
                barbed_charges >= 2 || bw_rem < GCD || barbed_dot_rem < SimDuration(2_000),
            ),
            self.pick(BESTIAL_WRATH, true),
            self.pick(WILD_THRASH, self.enemies > 1),
            self.pick(
                KILL_COMMAND,
                self.obs.has_buff(HOWL_READY_BUFF) || kc_charges >= 2 || na_up || !apex_r4,
            ),
            self.pick(COBRA_SHOT, self.obs.has_buff(HOGSTRIDER_BUFF)),
            self.pick(
                BARBED_SHOT,
                (self.focus < 75.0 && !self.obs.has_buff(SERPENTINE_STRIKES_TALENT))
                    || self.obs.has_buff(SERPENTINE_STRIKES_TALENT),
            ),
            self.pick(COBRA_SHOT, bw_rem > GCD),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `cleave` list (Pack Leader multi-target).
    fn cleave(&self) -> Option<SpellId> {
        let bc_talented = self.obs.has_buff(BEAST_CLEAVE_TALENT);
        let bc_rem = self.obs.buff_remaining(BEAST_CLEAVE_BUFF);
        let bc_ok = !bc_talented || bc_rem > SimDuration(400);
        let apex_r4 = self.obs.has_buff(NATURES_ALLY_R4_TALENT);
        [
            self.pick(WILD_THRASH, bc_talented && bc_rem < SimDuration(1_500)),
            self.pick(BARBED_SHOT, self.obs.charges(BARBED_SHOT) >= 2),
            self.pick(BESTIAL_WRATH, bc_ok || !self.known(WILD_THRASH)),
            self.pick(WILD_THRASH, !bc_talented),
            self.pick(
                KILL_COMMAND,
                (self.obs.has_buff(NATURES_ALLY_BUFF)
                    || self.obs.has_buff(HOWL_READY_BUFF)
                    || !apex_r4)
                    && bc_ok,
            ),
            self.pick(BARBED_SHOT, bc_ok),
            self.pick(COBRA_SHOT, bc_ok),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `drst` list (Dark Ranger single-target).
    fn drst(&self) -> Option<SpellId> {
        let bw_rem = self.cd(BESTIAL_WRATH);
        let wf_up = self.obs.has_buff(WITHERING_FIRE_BUFF);
        let wf_rem = self.obs.buff_remaining(WITHERING_FIRE_BUFF);
        let apex_r4 = self.obs.has_buff(NATURES_ALLY_R4_TALENT);
        let target_exec = self
            .obs
            .target
            .as_ref()
            .is_some_and(|t| t.health_pct <= 0.2 || t.health_pct >= 0.8);
        let ba_usable = target_exec || self.obs.has_buff(DEATHBLOW_BUFF) || wf_up;
        [
            self.pick(BARBED_SHOT, bw_rem < SimDuration(3_000)),
            self.pick(BESTIAL_WRATH, true),
            self.pick(BLACK_ARROW, wf_up && self.obs.charges(KILL_COMMAND) < 2),
            self.pick(
                KILL_COMMAND,
                self.obs.has_buff(NATURES_ALLY_BUFF)
                    || self.obs.charges(KILL_COMMAND) >= 2
                    || !apex_r4,
            ),
            self.pick(WAILING_ARROW, wf_rem < SimDuration(4_000)),
            self.pick(
                COBRA_SHOT,
                self.obs.has_buff(KILLER_COBRA_TALENT)
                    && self.obs.has_buff(BESTIAL_WRATH_BUFF)
                    && self.obs.charges(BARBED_SHOT) < 2,
            ),
            self.pick(BLACK_ARROW, ba_usable),
            self.pick(BARBED_SHOT, true),
            self.pick(WAILING_ARROW, true),
            self.pick(COBRA_SHOT, bw_rem > GCD),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `drcleave` list (Dark Ranger multi-target).
    fn drcleave(&self) -> Option<SpellId> {
        let bc_talented = self.obs.has_buff(BEAST_CLEAVE_TALENT);
        let bc_rem = self.obs.buff_remaining(BEAST_CLEAVE_BUFF);
        let wf_up = self.obs.has_buff(WITHERING_FIRE_BUFF);
        let apex_r4 = self.obs.has_buff(NATURES_ALLY_R4_TALENT);
        [
            self.pick(
                BLACK_ARROW,
                bc_rem < GCD && self.cd(BESTIAL_WRATH) < GCD && self.enemies > 2,
            ),
            self.pick(BESTIAL_WRATH, bc_rem > SimDuration::ZERO || !bc_talented),
            self.pick(WILD_THRASH, bc_rem < SimDuration(1_500) || !bc_talented),
            self.pick(
                KILL_COMMAND,
                self.obs.has_buff(NATURES_ALLY_BUFF) || !apex_r4,
            ),
            self.pick(BARBED_SHOT, self.obs.charges(BARBED_SHOT) >= 2),
            self.pick(BLACK_ARROW, wf_up),
            self.pick(WAILING_ARROW, true),
            self.pick(BARBED_SHOT, true),
            self.pick(BLACK_ARROW, true),
            self.pick(COBRA_SHOT, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Beast Mastery Hunter priority list (Pack Leader & Dark Ranger).
pub fn beast_mastery(d: &Decision<SeatObs>) -> Choice {
    let bm = Bm::new(d);
    let cleave_mode = bm.enemies > 2 || (bm.obs.has_buff(BEAST_CLEAVE_TALENT) && bm.enemies > 1);
    let rot = match (bm.dark_ranger, cleave_mode) {
        (true, true) => bm.drcleave(),
        (true, false) => bm.drst(),
        (false, true) => bm.cleave(),
        (false, false) => bm.st(),
    };
    rot.map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
