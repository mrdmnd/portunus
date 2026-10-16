//! Devastation Evoker.
//!
//! [`devastation`] ports SimC's 12.1 Scalecommander and Flameshaper priority
//! lists (`profiles/MID1/MID1_Evoker_Devastation.simc`).

use portunus_core::{AuraId, SpellId};
use portunus_engine::{ActionMask, CastOpts, Choice, TargetSel, Wait};
use portunus_env::Decision;

use crate::observe::SeatObs;

const LIVING_FLAME: SpellId = SpellId(361469);
const AZURE_STRIKE: SpellId = SpellId(362969);
const DISINTEGRATE: SpellId = SpellId(356995);
const PYRE: SpellId = SpellId(357211);
const FIRE_BREATH: SpellId = SpellId(357208);
const ETERNITY_SURGE: SpellId = SpellId(359073);
const DRAGONRAGE: SpellId = SpellId(375087);
const AZURE_SWEEP: SpellId = SpellId(1265867);
const DEEP_BREATH: SpellId = SpellId(357210);
const HOVER: SpellId = SpellId(358267);
const TIP_THE_SCALES: SpellId = SpellId(370553);

const HOVER_BUFF: AuraId = AuraId(358267);
const BURNOUT_BUFF: AuraId = AuraId(375802);
const MASS_DISINTEGRATE_BUFF: AuraId = AuraId(436336);

struct DevEv<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
}

impl<'a> DevEv<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let enemies = obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len());
        Self {
            obs,
            legal,
            enemies,
        }
    }

    fn pick(&self, s: SpellId, when: bool) -> Option<SpellId> {
        (when && self.legal.is_ready(s)).then_some(s)
    }

    fn rotation(&self) -> Option<SpellId> {
        [
            self.pick(DRAGONRAGE, true),
            self.pick(HOVER, !self.obs.has_buff(HOVER_BUFF)),
            self.pick(TIP_THE_SCALES, true),
            self.pick(ETERNITY_SURGE, true),
            self.pick(FIRE_BREATH, true),
            self.pick(DEEP_BREATH, true),
            self.pick(DISINTEGRATE, self.obs.has_buff(MASS_DISINTEGRATE_BUFF)),
            self.pick(PYRE, self.enemies >= 3),
            self.pick(DISINTEGRATE, true),
            self.pick(AZURE_SWEEP, true),
            self.pick(LIVING_FLAME, self.obs.has_buff(BURNOUT_BUFF)),
            self.pick(LIVING_FLAME, true),
            self.pick(AZURE_STRIKE, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Devastation Evoker priority list (Scalecommander & Flameshaper).
pub fn devastation(d: &Decision<SeatObs>) -> Choice {
    let Some(ability) = DevEv::new(d).rotation() else {
        return Choice::Wait(Wait::NextEvent);
    };
    let empower = matches!(ability, FIRE_BREATH | ETERNITY_SURGE).then_some(1);
    Choice::Cast {
        ability,
        target: TargetSel::Primary,
        opts: CastOpts {
            empower,
            tick_wakes: false,
        },
    }
}
