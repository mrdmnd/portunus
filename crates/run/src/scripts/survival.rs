//! Survival Hunter.
//!
//! [`survival`] ports SimC's 12.1 Pack Leader and Sentinel priority lists
//! (`profiles/MID2/MID2_Hunter_Survival.simc`).

use portunus_core::{AuraId, HeroTreeId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, MoveGoal, Wait};
use portunus_env::Decision;
use portunus_gamedata::spec::MELEE_RANGE;

use super::GCD;
use crate::observe::{SeatObs, FOREVER};

const KILL_COMMAND: SpellId = SpellId(259489);
const WILDFIRE_BOMB: SpellId = SpellId(259495);
const RAPTOR_STRIKE: SpellId = SpellId(186270);
const RAPTOR_SWIPE: SpellId = SpellId(1262293);
const BOOMSTICK: SpellId = SpellId(1261193);
const TAKEDOWN: SpellId = SpellId(1250646);
const MOONLIGHT_CHAKRAM: SpellId = SpellId(1264949);

const TIP_OF_THE_SPEAR_BUFF: AuraId = AuraId(260286);
const RAPTOR_SWIPE_BUFF: AuraId = AuraId(1273155);
const HOWL_READY_BUFF: AuraId = AuraId(471878);

const SENTINELS_MARK_DEBUFF: AuraId = AuraId(1253601);

const TWIN_FANGS_TALENT: AuraId = AuraId(1272139);

struct Sv<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
    pack_leader: bool,
    tots: u8,
    twin_fangs: bool,
}

impl<'a> Sv<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let enemies = obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len());
        let pack_leader = obs.hero_tree == Some(HeroTreeId(43));
        Self {
            obs,
            legal,
            enemies,
            pack_leader,
            tots: obs.buff_stacks(TIP_OF_THE_SPEAR_BUFF),
            twin_fangs: obs.has_buff(TWIN_FANGS_TALENT),
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

    fn raptor(&self, when: bool) -> Option<SpellId> {
        self.pick(RAPTOR_SWIPE, when)
            .or_else(|| self.pick(RAPTOR_STRIKE, when))
    }

    fn approach(&self) -> Option<Choice> {
        let t = self.obs.target.as_ref()?;
        (self.legal.can_move && self.obs.movement.is_none() && t.distance > MELEE_RANGE).then_some(
            Choice::Move(MoveGoal::Approach {
                target: t.actor,
                within: MELEE_RANGE,
            }),
        )
    }

    /// SimC's `plst` list (Pack Leader single-target).
    fn plst(&self) -> Option<SpellId> {
        let td_rem = self.cd(TAKEDOWN);
        [
            self.pick(
                KILL_COMMAND,
                self.tots < 2 && self.obs.has_buff(HOWL_READY_BUFF),
            ),
            self.pick(
                KILL_COMMAND,
                td_rem < GCD && self.tots < 2 && !self.twin_fangs,
            ),
            self.pick(
                TAKEDOWN,
                (self.tots > 0 && !self.twin_fangs) || (self.tots == 0 && self.twin_fangs),
            ),
            self.pick(RAPTOR_SWIPE, self.tots > 0),
            self.pick(WILDFIRE_BOMB, self.tots > 0),
            self.pick(BOOMSTICK, self.tots > 0),
            self.raptor(self.tots > 0 || !self.obs.has_buff(RAPTOR_SWIPE_BUFF)),
            self.pick(KILL_COMMAND, td_rem > SimDuration::ZERO),
            self.pick(TAKEDOWN, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `plcleave` list (Pack Leader multi-target).
    fn plcleave(&self) -> Option<SpellId> {
        let td_rem = self.cd(TAKEDOWN);
        [
            self.pick(
                KILL_COMMAND,
                self.tots < 2 && self.obs.has_buff(HOWL_READY_BUFF),
            ),
            self.pick(
                KILL_COMMAND,
                td_rem < GCD && self.tots < 2 && !self.twin_fangs,
            ),
            self.pick(
                TAKEDOWN,
                (self.tots > 0 && !self.twin_fangs) || (self.tots == 0 && self.twin_fangs),
            ),
            self.pick(WILDFIRE_BOMB, self.obs.charges(WILDFIRE_BOMB) >= 2),
            self.pick(BOOMSTICK, self.tots > 0),
            self.pick(WILDFIRE_BOMB, self.tots > 0),
            self.raptor(self.tots > 0 || !self.obs.has_buff(RAPTOR_SWIPE_BUFF)),
            self.pick(KILL_COMMAND, td_rem > SimDuration::ZERO),
            self.pick(WILDFIRE_BOMB, true),
            self.pick(TAKEDOWN, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `sentst` list (Sentinel single-target).
    fn sentst(&self) -> Option<SpellId> {
        let td_rem = self.cd(TAKEDOWN);
        [
            self.pick(
                KILL_COMMAND,
                self.tots == 0 && (td_rem > SimDuration::ZERO || !self.twin_fangs),
            ),
            self.pick(BOOMSTICK, true),
            self.pick(
                WILDFIRE_BOMB,
                self.obs.target_aura(SENTINELS_MARK_DEBUFF).is_some()
                    || self.obs.charges(WILDFIRE_BOMB) >= 2,
            ),
            self.pick(
                KILL_COMMAND,
                td_rem < GCD && self.tots < 2 && !self.twin_fangs,
            ),
            self.pick(
                TAKEDOWN,
                (self.tots > 0 && !self.twin_fangs) || (self.tots == 0 && self.twin_fangs),
            ),
            self.pick(MOONLIGHT_CHAKRAM, true),
            self.pick(RAPTOR_SWIPE, self.tots > 0),
            self.raptor(true),
            self.pick(KILL_COMMAND, td_rem > SimDuration::ZERO),
            self.pick(WILDFIRE_BOMB, true),
            self.pick(TAKEDOWN, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `sentcleave` list (Sentinel multi-target).
    fn sentcleave(&self) -> Option<SpellId> {
        let td_rem = self.cd(TAKEDOWN);
        [
            self.pick(
                KILL_COMMAND,
                self.tots == 0 && (td_rem > SimDuration::ZERO || !self.twin_fangs),
            ),
            self.pick(BOOMSTICK, self.tots > 0),
            self.pick(WILDFIRE_BOMB, self.tots > 0),
            self.pick(
                KILL_COMMAND,
                td_rem < GCD && self.tots < 2 && !self.twin_fangs,
            ),
            self.pick(
                TAKEDOWN,
                (self.tots > 0 && !self.twin_fangs) || (self.tots == 0 && self.twin_fangs),
            ),
            self.pick(MOONLIGHT_CHAKRAM, self.tots > 0),
            self.raptor(
                (self.tots > 0 && self.obs.has_buff(RAPTOR_SWIPE_BUFF))
                    || !self.obs.has_buff(RAPTOR_SWIPE_BUFF),
            ),
            self.pick(KILL_COMMAND, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Survival Hunter priority list (Pack Leader & Sentinel).
pub fn survival(d: &Decision<SeatObs>) -> Choice {
    let sv = Sv::new(d);
    if let Some(move_in) = sv.approach() {
        return move_in;
    }
    let rot = match (sv.pack_leader, sv.enemies > 2) {
        (true, true) => sv.plcleave(),
        (true, false) => sv.plst(),
        (false, true) => sv.sentcleave(),
        (false, false) => sv.sentst(),
    };
    rot.map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
