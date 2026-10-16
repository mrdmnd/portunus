//! Havoc Demon Hunter.
//!
//! [`havoc`] ports SimC's 12.1 Fel-Scarred and Aldrachi Reaver priority
//! lists (`profiles/MID2/MID2_Demon_Hunter_Havoc.simc`).

use portunus_core::{AuraId, SpellId};
use portunus_engine::{ActionMask, Choice, MoveGoal, Wait};
use portunus_env::Decision;
use portunus_gamedata::spec::MELEE_RANGE;
use portunus_gamedata::stats::ResourceKind;

use crate::observe::SeatObs;

const DEMONS_BITE: SpellId = SpellId(162243);
const CHAOS_STRIKE: SpellId = SpellId(162794);
const ANNIHILATION: SpellId = SpellId(201427);
const BLADE_DANCE: SpellId = SpellId(188499);
const DEATH_SWEEP: SpellId = SpellId(210152);
const EYE_BEAM: SpellId = SpellId(198013);
const IMMOLATION_AURA: SpellId = SpellId(258920);
const FELBLADE: SpellId = SpellId(232893);
const THROW_GLAIVE: SpellId = SpellId(185123);
const REAVERS_GLAIVE: SpellId = SpellId(442294);
const VENGEFUL_RETREAT: SpellId = SpellId(198793);
const METAMORPHOSIS: SpellId = SpellId(191427);
const SIGIL_OF_FLAME: SpellId = SpellId(204596);
const THE_HUNT: SpellId = SpellId(370965);
const ESSENCE_BREAK: SpellId = SpellId(258860);
const GLAIVE_TEMPEST: SpellId = SpellId(1244557);

const UNBOUND_CHAOS_BUFF: AuraId = AuraId(347462);
const REAVERS_GLAIVE_READY_BUFF: AuraId = AuraId(442292);
const RENDING_STRIKE_BUFF: AuraId = AuraId(442624);
const GLAIVE_FLURRY_BUFF: AuraId = AuraId(442625);

const ESSENCE_BREAK_DEBUFF: AuraId = AuraId(320338);

struct Havoc<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    fury: f64,
}

impl<'a> Havoc<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let fury = obs.resource(ResourceKind::Fury);
        Self { obs, legal, fury }
    }

    fn pick(&self, s: SpellId, when: bool) -> Option<SpellId> {
        (when && self.legal.is_ready(s)).then_some(s)
    }

    fn dance(&self, when: bool) -> Option<SpellId> {
        self.pick(DEATH_SWEEP, when)
            .or_else(|| self.pick(BLADE_DANCE, when))
    }

    fn strike(&self, when: bool) -> Option<SpellId> {
        self.pick(ANNIHILATION, when)
            .or_else(|| self.pick(CHAOS_STRIKE, when))
    }

    fn rotation(&self) -> Option<SpellId> {
        let eb_up = self.obs.target_aura(ESSENCE_BREAK_DEBUFF).is_some();
        let rending = self.obs.has_buff(RENDING_STRIKE_BUFF);
        let flurry = self.obs.has_buff(GLAIVE_FLURRY_BUFF);
        [
            self.pick(VENGEFUL_RETREAT, true),
            self.pick(
                REAVERS_GLAIVE,
                self.obs.has_buff(REAVERS_GLAIVE_READY_BUFF) && !rending && !flurry,
            ),
            self.pick(
                THROW_GLAIVE,
                self.obs.has_buff(REAVERS_GLAIVE_READY_BUFF) && !rending && !flurry,
            ),
            self.strike(rending && !flurry),
            self.dance(flurry),
            self.pick(THE_HUNT, !self.obs.has_buff(REAVERS_GLAIVE_READY_BUFF)),
            self.pick(ESSENCE_BREAK, true),
            self.dance(eb_up),
            self.strike(eb_up),
            self.pick(EYE_BEAM, true),
            self.pick(METAMORPHOSIS, true),
            self.pick(FELBLADE, self.obs.has_buff(UNBOUND_CHAOS_BUFF)),
            self.dance(true),
            self.pick(GLAIVE_TEMPEST, true),
            self.pick(SIGIL_OF_FLAME, self.fury <= 90.0),
            self.pick(IMMOLATION_AURA, true),
            self.pick(FELBLADE, self.fury <= 80.0),
            self.strike(true),
            self.pick(FELBLADE, true),
            self.pick(DEMONS_BITE, true),
            self.pick(THROW_GLAIVE, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Havoc Demon Hunter priority list (Fel-Scarred & Aldrachi Reaver).
pub fn havoc(d: &Decision<SeatObs>) -> Choice {
    if let Some(t) = &d.obs.target {
        if t.distance > MELEE_RANGE && d.obs.movement.is_none() && d.legal.can_move {
            return Choice::Move(MoveGoal::Approach {
                target: t.actor,
                within: MELEE_RANGE,
            });
        }
    }
    Havoc::new(d)
        .rotation()
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
