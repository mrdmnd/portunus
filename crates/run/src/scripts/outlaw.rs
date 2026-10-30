//! Outlaw Rogue.
//!
//! [`outlaw`] ports SimC's 12.1 Trickster and Fatebound priority lists
//! (`profiles/MID2/MID2_Rogue_Outlaw.simc`).

use portunus_core::{AuraId, HeroTreeId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, MoveGoal, Wait};
use portunus_env::Decision;
use portunus_gamedata::spec::MELEE_RANGE;
use portunus_gamedata::stats::ResourceKind;

use super::GCD;
use crate::observe::SeatObs;

const SINISTER_STRIKE: SpellId = SpellId(193315);
const PISTOL_SHOT: SpellId = SpellId(185763);
const AMBUSH: SpellId = SpellId(8676);
const DISPATCH: SpellId = SpellId(2098);
const BETWEEN_THE_EYES: SpellId = SpellId(315341);
const ROLL_THE_BONES: SpellId = SpellId(1214909);
const KEEP_IT_ROLLING: SpellId = SpellId(381989);
const BLADE_FLURRY: SpellId = SpellId(13877);
const ADRENALINE_RUSH: SpellId = SpellId(13750);
const BLADE_RUSH: SpellId = SpellId(271877);
const KILLING_SPREE: SpellId = SpellId(51690);
const COUP_DE_GRACE: SpellId = SpellId(441776);
const VANISH: SpellId = SpellId(1856);
const STEALTH: SpellId = SpellId(1784);

const ADRENALINE_RUSH_BUFF: AuraId = AuraId(13750);
const BLADE_FLURRY_BUFF: AuraId = AuraId(13877);
const OPPORTUNITY_BUFF: AuraId = AuraId(195627);
const AUDACITY_BUFF: AuraId = AuraId(386270);
const RTB_STAGE_1_BUFF: AuraId = AuraId(1214934);
const RTB_STAGE_2_BUFF: AuraId = AuraId(1214935);
const LOADED_DICE_BUFF: AuraId = AuraId(256170);

const DEFT_MANEUVERS_TALENT: AuraId = AuraId(381878);
const HIDDEN_OPPORTUNITY_TALENT: AuraId = AuraId(383281);

struct Outlaw<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
    cp: f64,
    cp_deficit: f64,
    fatebound: bool,
}

impl<'a> Outlaw<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let enemies = obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len());
        let cp = obs.resource(ResourceKind::ComboPoints);
        let cp_max = obs
            .resource_max
            .get(&ResourceKind::ComboPoints)
            .copied()
            .unwrap_or(7.0);
        let fatebound = obs.hero_tree == Some(HeroTreeId(52));
        Self {
            obs,
            legal,
            enemies,
            cp,
            cp_deficit: (cp_max - cp).max(0.0),
            fatebound,
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
        let rtb_up = self.obs.has_buff(RTB_STAGE_1_BUFF);
        let rtb_2 = self.obs.has_buff(RTB_STAGE_2_BUFF);
        let ld_up = self.obs.has_buff(LOADED_DICE_BUFF);
        [
            self.pick(STEALTH, true),
            self.pick(ADRENALINE_RUSH, !self.obs.has_buff(ADRENALINE_RUSH_BUFF)),
            self.pick(
                BLADE_FLURRY,
                self.enemies >= 2 && self.obs.buff_remaining(BLADE_FLURRY_BUFF) < GCD,
            ),
            self.pick(KEEP_IT_ROLLING, rtb_2),
            self.pick(ROLL_THE_BONES, !rtb_up || (!rtb_2 && ld_up)),
            self.pick(BLADE_RUSH, true),
            self.pick(
                VANISH,
                self.cp < 5.0
                    && self.obs.has_buff(HIDDEN_OPPORTUNITY_TALENT)
                    && !self.obs.has_buff(AUDACITY_BUFF)
                    && !self.obs.has_buff(OPPORTUNITY_BUFF),
            ),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `finish` list.
    fn finish(&self) -> Option<SpellId> {
        [
            self.pick(BETWEEN_THE_EYES, true),
            self.pick(KILLING_SPREE, true),
            self.pick(COUP_DE_GRACE, true),
            self.pick(DISPATCH, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `build` list.
    fn build(&self) -> Option<SpellId> {
        let opp_stacks = self.obs.buff_stacks(OPPORTUNITY_BUFF);
        let opp_rem = self.obs.buff_remaining(OPPORTUNITY_BUFF);
        [
            self.pick(AMBUSH, true),
            self.pick(
                BLADE_FLURRY,
                self.obs.has_buff(DEFT_MANEUVERS_TALENT) && self.enemies >= 3,
            ),
            self.pick(
                PISTOL_SHOT,
                opp_stacks > 0
                    && (opp_stacks >= 6 || opp_rem < SimDuration(2_000) || self.cp_deficit >= 2.0),
            ),
            self.pick(SINISTER_STRIKE, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Outlaw Rogue priority list (Trickster & Fatebound).
pub fn outlaw(d: &Decision<SeatObs>) -> Choice {
    let o = Outlaw::new(d);
    if let Some(move_in) = o.approach() {
        return move_in;
    }
    let finish_threshold = if o.fatebound && !o.legal.is_ready(BETWEEN_THE_EYES) {
        5.0
    } else {
        6.0
    };
    let act = super::use_buff_trinket(d, true)
        .or_else(|| o.cds())
        .or_else(|| {
            if o.cp >= finish_threshold {
                o.finish()
            } else {
                o.build()
            }
        });
    act.map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
