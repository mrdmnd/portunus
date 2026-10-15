//! Subtlety Rogue.
//!
//! [`subtlety`] ports SimC's 12.1 Deathstalker and Trickster priority lists
//! (`profiles/MID2/MID2_Rogue_Subtlety.simc`).

use portunus_core::{AuraId, SpellId};
use portunus_engine::{ActionMask, Choice, MoveGoal, Wait};
use portunus_env::Decision;
use portunus_gamedata::spec::MELEE_RANGE;
use portunus_gamedata::stats::ResourceKind;

use crate::observe::SeatObs;

const BACKSTAB: SpellId = SpellId(53);
const GLOOMBLADE: SpellId = SpellId(200758);
const SHADOWSTRIKE: SpellId = SpellId(185438);
const SHURIKEN_STORM: SpellId = SpellId(197835);
const EVISCERATE: SpellId = SpellId(196819);
const BLACK_POWDER: SpellId = SpellId(319175);
const SECRET_TECHNIQUE: SpellId = SpellId(280719);
const SHADOW_DANCE: SpellId = SpellId(185313);
const SHADOW_BLADES: SpellId = SpellId(121471);
const GOREMAWS_BITE: SpellId = SpellId(426591);
const SLICE_AND_DICE: SpellId = SpellId(315496);
const COUP_DE_GRACE: SpellId = SpellId(441776);
const VANISH: SpellId = SpellId(1856);
const STEALTH: SpellId = SpellId(1784);

const STEALTH_BUFF: AuraId = AuraId(1784);
const VANISH_BUFF: AuraId = AuraId(11327);
const SUBTERFUGE_BUFF: AuraId = AuraId(115192);
const SHADOW_DANCE_BUFF: AuraId = AuraId(185422);
const SHADOW_BLADES_BUFF: AuraId = AuraId(121471);
const SLICE_AND_DICE_BUFF: AuraId = AuraId(315496);
const PREMEDITATION_BUFF: AuraId = AuraId(343161);
const DARKEST_NIGHT_BUFF: AuraId = AuraId(457280);

struct Sub<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
    energy: f64,
    cp: f64,
    stealthed: bool,
    darkest_night: bool,
}

impl<'a> Sub<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let enemies = obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len());
        let energy = obs.resource(ResourceKind::Energy);
        let cp = obs.resource(ResourceKind::ComboPoints);
        let stealthed = obs.has_buff(STEALTH_BUFF)
            || obs.has_buff(VANISH_BUFF)
            || obs.has_buff(SUBTERFUGE_BUFF)
            || obs.has_buff(SHADOW_DANCE_BUFF);
        Self {
            obs,
            legal,
            enemies,
            energy,
            cp,
            stealthed,
            darkest_night: obs.has_buff(DARKEST_NIGHT_BUFF),
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

    /// SimC's `cds` list.
    fn cds(&self) -> Option<SpellId> {
        [
            self.pick(STEALTH, true),
            self.pick(
                GOREMAWS_BITE,
                self.obs.has_buff(SHADOW_DANCE_BUFF) || self.cp <= 4.0,
            ),
            self.pick(SHADOW_BLADES, !self.obs.has_buff(SHADOW_BLADES_BUFF)),
            self.pick(
                SHADOW_DANCE,
                !self.stealthed
                    && (self.obs.has_buff(SHADOW_BLADES_BUFF)
                        || self.legal.is_ready(SECRET_TECHNIQUE)
                        || self.darkest_night),
            ),
            self.pick(
                VANISH,
                !self.stealthed && self.energy >= 50.0 && self.cp <= 2.0,
            ),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `finish` list.
    fn finish(&self) -> Option<SpellId> {
        [
            self.pick(SLICE_AND_DICE, !self.obs.has_buff(SLICE_AND_DICE_BUFF)),
            self.pick(EVISCERATE, self.darkest_night),
            self.pick(SECRET_TECHNIQUE, true),
            self.pick(COUP_DE_GRACE, true),
            self.pick(BLACK_POWDER, self.enemies >= 2),
            self.pick(EVISCERATE, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `build` list.
    fn build(&self) -> Option<SpellId> {
        [
            self.pick(
                SHURIKEN_STORM,
                (self.obs.has_buff(SHADOW_DANCE_BUFF) && self.obs.has_buff(PREMEDITATION_BUFF))
                    || self.enemies > 1,
            ),
            self.pick(SHADOWSTRIKE, true),
            self.pick(GLOOMBLADE, true),
            self.pick(BACKSTAB, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Subtlety Rogue priority list (Deathstalker & Trickster).
pub fn subtlety(d: &Decision<SeatObs>) -> Choice {
    let s = Sub::new(d);
    if let Some(move_in) = s.approach() {
        return move_in;
    }
    let finish_threshold = if s.darkest_night { 7.0 } else { 6.0 };
    let act = s.cds().or_else(|| {
        if s.cp >= finish_threshold {
            s.finish()
        } else {
            s.build()
        }
    });
    act.map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
