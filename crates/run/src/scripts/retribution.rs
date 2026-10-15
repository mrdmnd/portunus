//! Retribution Paladin.
//!
//! [`retribution`] ports SimC's 12.1 Templar and Herald of the Sun priority
//! lists (`profiles/MID2/MID2_Paladin_Retribution.simc`).

use portunus_core::{AuraId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, MoveGoal, Wait};
use portunus_env::Decision;
use portunus_gamedata::spec::MELEE_RANGE;
use portunus_gamedata::stats::ResourceKind;

use crate::observe::SeatObs;

const CRUSADER_STRIKE: SpellId = SpellId(35395);
const TEMPLAR_STRIKE: SpellId = SpellId(407480);
const TEMPLAR_SLASH: SpellId = SpellId(406647);
const JUDGMENT: SpellId = SpellId(20271);
const HAMMER_OF_WRATH: SpellId = SpellId(24275);
const BLADE_OF_JUSTICE: SpellId = SpellId(184575);
const TEMPLARS_VERDICT: SpellId = SpellId(85256);
const FINAL_VERDICT: SpellId = SpellId(383328);
const DIVINE_STORM: SpellId = SpellId(53385);
const WAKE_OF_ASHES: SpellId = SpellId(255937);
const EXECUTION_SENTENCE: SpellId = SpellId(343527);
const AVENGING_WRATH: SpellId = SpellId(31884);
const DIVINE_TOLL: SpellId = SpellId(375576);
const HAMMER_OF_LIGHT: SpellId = SpellId(427453);
const CONSECRATION: SpellId = SpellId(26573);

const AVENGING_WRATH_BUFF: AuraId = AuraId(31884);
const DIVINE_PURPOSE_BUFF: AuraId = AuraId(408458);
const EMPYREAN_POWER_BUFF: AuraId = AuraId(326733);
const EMPYREAN_LEGACY_BUFF: AuraId = AuraId(387178);
const ART_OF_WAR_BUFF: AuraId = AuraId(231843);
const HAMMER_OF_LIGHT_READY: AuraId = AuraId(427441);
const HAMMER_OF_LIGHT_FREE: AuraId = AuraId(433732);
const SHAKE_THE_HEAVENS_BUFF: AuraId = AuraId(431536);
const RADIANT_GLORY_TALENT: AuraId = AuraId(458359);
const WALK_INTO_LIGHT_TALENT: AuraId = AuraId(1263782);

struct Ret<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
    hp: f64,
}

impl<'a> Ret<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        Self {
            obs,
            legal,
            enemies: obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len()),
            hp: obs.resource(ResourceKind::HolyPower),
        }
    }

    fn pick(&self, s: SpellId, when: bool) -> Option<SpellId> {
        (when && self.legal.is_ready(s)).then_some(s)
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

    /// SimC's `cooldowns` list (12.1).
    fn cooldowns(&self) -> Option<SpellId> {
        let radiant_glory = self.obs.has_buff(RADIANT_GLORY_TALENT);
        let wings_up = self.obs.has_buff(AVENGING_WRATH_BUFF);
        let woa_ready_soon = self.obs.cooldown_remaining(WAKE_OF_ASHES) <= SimDuration(1_500);
        [
            self.pick(
                EXECUTION_SENTENCE,
                (!wings_up && woa_ready_soon)
                    || wings_up
                    || self.obs.combat_time.is_some_and(|t| t < SimDuration(5_000)),
            ),
            self.pick(
                AVENGING_WRATH,
                !radiant_glory && (self.hp >= 3.0 || woa_ready_soon),
            ),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `finishers` list (12.1).
    fn finishers(&self) -> Option<SpellId> {
        let ep_up = self.obs.has_buff(EMPYREAN_POWER_BUFF);
        let el_up = self.obs.has_buff(EMPYREAN_LEGACY_BUFF);
        let sth_up = self.obs.has_buff(SHAKE_THE_HEAVENS_BUFF);
        let hol_free = self.obs.has_buff(HAMMER_OF_LIGHT_FREE);
        [
            self.pick(HAMMER_OF_LIGHT, hol_free || sth_up || !sth_up),
            self.pick(DIVINE_STORM, (self.enemies >= 2 || ep_up) && !el_up),
            self.pick(FINAL_VERDICT, true),
            self.pick(TEMPLARS_VERDICT, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `generators` list (12.1).
    fn generators(&self) -> Option<SpellId> {
        let hol_free_rem = self.obs.buff_remaining(HAMMER_OF_LIGHT_FREE);
        let hol_ready_rem = self.obs.buff_remaining(HAMMER_OF_LIGHT_READY);
        let dp_up = self.obs.has_buff(DIVINE_PURPOSE_BUFF);
        let hol_ready = self.obs.has_buff(HAMMER_OF_LIGHT_READY);
        let urgent_finisher = self.hp >= 5.0
            || (hol_free_rem > SimDuration::ZERO && hol_free_rem <= SimDuration(1_500))
            || (hol_ready_rem > SimDuration::ZERO && hol_ready_rem <= SimDuration(1_500))
            || (dp_up && hol_ready);
        if urgent_finisher {
            if let Some(s) = self.finishers() {
                return Some(s);
            }
        }

        if let Some(s) = self.pick(
            WAKE_OF_ASHES,
            (self.hp <= 2.0 || (self.hp == 3.0 && dp_up)) && !hol_ready,
        ) {
            return Some(s);
        }

        if let Some(s) = self.pick(DIVINE_TOLL, self.hp <= 3.0) {
            return Some(s);
        }

        if let Some(s) = self.pick(BLADE_OF_JUSTICE, self.obs.buff_stacks(ART_OF_WAR_BUFF) >= 2) {
            return Some(s);
        }

        if let Some(s) = self.pick(
            HAMMER_OF_WRATH,
            self.obs.charges(HAMMER_OF_WRATH) >= 2 && self.obs.has_buff(WALK_INTO_LIGHT_TALENT),
        ) {
            return Some(s);
        }

        if let Some(s) = self.finishers() {
            return Some(s);
        }

        [
            self.pick(HAMMER_OF_WRATH, true),
            self.pick(BLADE_OF_JUSTICE, true),
            self.pick(JUDGMENT, true),
            self.pick(TEMPLAR_SLASH, true),
            self.pick(TEMPLAR_STRIKE, true),
            self.pick(CRUSADER_STRIKE, true),
            self.pick(CONSECRATION, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Retribution Paladin priority list (Templar & Herald of the Sun).
pub fn retribution(d: &Decision<SeatObs>) -> Choice {
    let r = Ret::new(d);
    if let Some(move_in) = r.approach() {
        return move_in;
    }
    r.cooldowns()
        .or_else(|| r.generators())
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
