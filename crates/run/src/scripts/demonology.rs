//! Demonology Warlock.
//!
//! [`demonology`] ports SimC's 12.1 Diabolist and Soul Harvester priority
//! lists (`profiles/MID2/MID2_Warlock_Demonology.simc`).

use portunus_core::{AuraId, SpellId};
use portunus_engine::{ActionMask, Choice, Wait};
use portunus_env::Decision;
use portunus_gamedata::stats::ResourceKind;

use crate::observe::SeatObs;

const SHADOW_BOLT: SpellId = SpellId(686);
const DEMONBOLT: SpellId = SpellId(264178);
const HAND_OF_GULDAN: SpellId = SpellId(1250273);
const CALL_DREADSTALKERS: SpellId = SpellId(104316);
const IMPLOSION: SpellId = SpellId(196277);
const POWER_SIPHON: SpellId = SpellId(264130);
const SUMMON_DEMONIC_TYRANT: SpellId = SpellId(265187);
const GRIMOIRE_IMP_LORD: SpellId = SpellId(1276452);
const GRIMOIRE_FEL_RAVAGER: SpellId = SpellId(1276467);
const SUMMON_DOOMGUARD: SpellId = SpellId(1276672);
const SUMMON_VILEFIEND: SpellId = SpellId(1251778);
const RUINATION: SpellId = SpellId(434635);
const INFERNAL_BOLT: SpellId = SpellId(434506);

const DEMONIC_CORE_BUFF: AuraId = AuraId(264173);
const WILD_IMPS_BUFF: AuraId = AuraId(296553);
const DOMINION_OF_ARGUS_BUFF: AuraId = AuraId(1276165);

struct Demo<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
    shards: f64,
    cores: u8,
    imps: u8,
}

impl<'a> Demo<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let enemies = obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len());
        let shards = obs.resource(ResourceKind::SoulShards);
        Self {
            obs,
            legal,
            enemies,
            shards,
            cores: obs.buff_stacks(DEMONIC_CORE_BUFF),
            imps: obs.buff_stacks(WILD_IMPS_BUFF),
        }
    }

    fn pick(&self, s: SpellId, when: bool) -> Option<SpellId> {
        (when && self.legal.is_ready(s)).then_some(s)
    }

    fn rotation(&self) -> Option<SpellId> {
        [
            self.pick(POWER_SIPHON, self.cores <= 1),
            self.pick(RUINATION, true),
            self.pick(
                HAND_OF_GULDAN,
                self.obs.has_buff(DOMINION_OF_ARGUS_BUFF) && self.shards >= 1.0,
            ),
            self.pick(GRIMOIRE_IMP_LORD, true),
            self.pick(GRIMOIRE_FEL_RAVAGER, true),
            self.pick(SUMMON_DOOMGUARD, true),
            self.pick(SUMMON_VILEFIEND, true),
            self.pick(CALL_DREADSTALKERS, true),
            self.pick(SUMMON_DEMONIC_TYRANT, true),
            self.pick(IMPLOSION, self.imps >= 6 && self.enemies >= 2),
            self.pick(HAND_OF_GULDAN, self.shards >= 3.0),
            self.pick(INFERNAL_BOLT, self.shards < 3.0),
            self.pick(DEMONBOLT, self.shards < 4.0 && self.cores >= 1),
            self.pick(SHADOW_BOLT, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Demonology Warlock priority list (Diabolist & Soul Harvester).
pub fn demonology(d: &Decision<SeatObs>) -> Choice {
    Demo::new(d)
        .rotation()
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
