//! Shadow Priest.
//!
//! [`shadow`] ports SimC's 12.1 Voidweaver and Archon priority lists
//! (`profiles/MID2/MID2_Priest_Shadow.simc`).

use portunus_core::{AuraId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, Wait};
use portunus_env::Decision;
use portunus_gamedata::stats::ResourceKind;

use crate::observe::SeatObs;

const SHADOW_WORD_PAIN: SpellId = SpellId(589);
const VAMPIRIC_TOUCH: SpellId = SpellId(34914);
const MIND_BLAST: SpellId = SpellId(8092);
const VOID_BLAST: SpellId = SpellId(450405);
const MIND_FLAY: SpellId = SpellId(15407);
const MIND_FLAY_INSANITY: SpellId = SpellId(391403);
const SHADOW_WORD_DEATH: SpellId = SpellId(32379);
const SHADOW_WORD_MADNESS: SpellId = SpellId(335467);
const TENTACLE_SLAM: SpellId = SpellId(1227280);
const VOIDFORM: SpellId = SpellId(228260);
const VOID_VOLLEY: SpellId = SpellId(1242173);
const SHADOWFIEND: SpellId = SpellId(34433);
const MINDBENDER: SpellId = SpellId(1230339);
const VOIDWRAITH: SpellId = SpellId(451234);
const VOID_TORRENT: SpellId = SpellId(263165);
const HALO: SpellId = SpellId(120644);

const VOIDFORM_BUFF: AuraId = AuraId(194249);
const MIND_DEVOURER_BUFF: AuraId = AuraId(373204);
const ENTROPIC_RIFT_BUFF: AuraId = AuraId(447444);

const SWP_DOT: AuraId = AuraId(589);
const VT_DOT: AuraId = AuraId(34914);
const MADNESS_DOT: AuraId = AuraId(335467);

struct Shadow<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    insanity: f64,
    target_hp: f64,
}

impl<'a> Shadow<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let insanity = obs.resource(ResourceKind::Insanity);
        let target_hp = obs.target.as_ref().map_or(1.0, |t| t.health_pct);
        Self {
            obs,
            legal,
            insanity,
            target_hp,
        }
    }

    fn pick(&self, s: SpellId, when: bool) -> Option<SpellId> {
        (when && self.legal.is_ready(s)).then_some(s)
    }

    fn dot_refreshable(&self, aura: AuraId, threshold_ms: u32) -> bool {
        self.obs
            .target_aura(aura)
            .is_none_or(|a| a.remaining.is_some_and(|r| r <= SimDuration(threshold_ms)))
    }

    fn rotation(&self) -> Option<SpellId> {
        let vt_refresh = self.dot_refreshable(VT_DOT, 6300);
        let swp_refresh = self.dot_refreshable(SWP_DOT, 4800);
        let madness_refresh = self.dot_refreshable(MADNESS_DOT, 1800);
        [
            self.pick(TENTACLE_SLAM, vt_refresh),
            self.pick(VAMPIRIC_TOUCH, vt_refresh),
            self.pick(SHADOW_WORD_PAIN, swp_refresh),
            self.pick(VOIDWRAITH, true),
            self.pick(MINDBENDER, true),
            self.pick(SHADOWFIEND, true),
            self.pick(HALO, true),
            self.pick(VOIDFORM, true),
            self.pick(
                SHADOW_WORD_MADNESS,
                madness_refresh
                    || self.insanity >= 90.0
                    || self.obs.has_buff(MIND_DEVOURER_BUFF)
                    || self.obs.has_buff(ENTROPIC_RIFT_BUFF)
                    || self.obs.has_buff(VOIDFORM_BUFF),
            ),
            self.pick(VOID_TORRENT, true),
            self.pick(VOID_VOLLEY, true),
            self.pick(VOID_BLAST, true),
            self.pick(MIND_BLAST, true),
            self.pick(MIND_FLAY_INSANITY, true),
            self.pick(TENTACLE_SLAM, true),
            self.pick(SHADOW_WORD_DEATH, self.target_hp <= 0.35),
            self.pick(MIND_FLAY, true),
            self.pick(SHADOW_WORD_DEATH, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Shadow Priest priority list (Voidweaver & Archon).
pub fn shadow(d: &Decision<SeatObs>) -> Choice {
    Shadow::new(d)
        .rotation()
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
