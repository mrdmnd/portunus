//! Augmentation Evoker.
//!
//! [`augmentation`] ports SimC's 12.1 Scalecommander and Chronowarden
//! priority lists.

use portunus_core::{AuraId, SimDuration, SpellId};
use portunus_engine::{ActionMask, CastOpts, Choice, TargetSel, Wait};
use portunus_env::Decision;

use crate::observe::SeatObs;

const LIVING_FLAME: SpellId = SpellId(361469);
const AZURE_STRIKE: SpellId = SpellId(362969);
const FIRE_BREATH: SpellId = SpellId(357208);
const DEEP_BREATH: SpellId = SpellId(357210);
const HOVER: SpellId = SpellId(358267);
const TIP_THE_SCALES: SpellId = SpellId(370553);
const EBON_MIGHT: SpellId = SpellId(395152);
const ERUPTION: SpellId = SpellId(395160);
const UPHEAVAL: SpellId = SpellId(396286);
const BREATH_OF_EONS: SpellId = SpellId(403631);
const BLISTERING_SCALES: SpellId = SpellId(360827);
const PRESCIENCE: SpellId = SpellId(409311);
const TIME_SKIP: SpellId = SpellId(404977);

const HOVER_BUFF: AuraId = AuraId(358267);
const BLISTERING_SCALES_BUFF: AuraId = AuraId(360827);
const PRESCIENCE_BUFF: AuraId = AuraId(410089);

struct AugEv<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
}

impl<'a> AugEv<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        Self {
            obs: &d.obs,
            legal: &d.legal,
        }
    }

    fn pick(&self, s: SpellId, when: bool) -> Option<SpellId> {
        (when && self.legal.is_ready(s)).then_some(s)
    }

    fn rotation(&self) -> Option<SpellId> {
        [
            self.pick(
                BLISTERING_SCALES,
                !self.obs.has_buff(BLISTERING_SCALES_BUFF),
            ),
            self.pick(PRESCIENCE, !self.obs.has_buff(PRESCIENCE_BUFF)),
            self.pick(EBON_MIGHT, true),
            self.pick(BREATH_OF_EONS, true),
            self.pick(HOVER, !self.obs.has_buff(HOVER_BUFF)),
            self.pick(TIP_THE_SCALES, true),
            self.pick(FIRE_BREATH, true),
            self.pick(UPHEAVAL, true),
            self.pick(DEEP_BREATH, true),
            self.pick(ERUPTION, true),
            self.pick(
                TIME_SKIP,
                self.obs.cooldown_remaining(EBON_MIGHT) > SimDuration(10_000)
                    && self.obs.cooldown_remaining(FIRE_BREATH) > SimDuration(10_000),
            ),
            self.pick(PRESCIENCE, self.obs.charges(PRESCIENCE) >= 2),
            self.pick(LIVING_FLAME, true),
            self.pick(AZURE_STRIKE, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Augmentation Evoker priority list (Scalecommander & Chronowarden).
pub fn augmentation(d: &Decision<SeatObs>) -> Choice {
    let Some(ability) = AugEv::new(d).rotation() else {
        return Choice::Wait(Wait::NextEvent);
    };
    let empower = matches!(ability, FIRE_BREATH | UPHEAVAL).then_some(1);
    Choice::Cast {
        ability,
        target: TargetSel::Primary,
        opts: CastOpts {
            empower,
            tick_wakes: false,
        },
    }
}
