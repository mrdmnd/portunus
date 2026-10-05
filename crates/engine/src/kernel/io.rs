//! What mechanics get to touch.

use portunus_core::{ActorId, AuraId, PetId, Seat, SimDuration, SpellId, StreamKey};
use portunus_gamedata::effect::CooldownChange;
use portunus_gamedata::stats::ResourceKind;
use portunus_gamedata::GameData;

use crate::mechanics::{AuraApplication, DamageEvent, EngineIo, HealEvent, TimerEvent};
use crate::state::{AuraRef, ListenerRef, StateView};
use crate::step::WakeReason;

use super::queue::Event;
use super::world::World;

pub(crate) struct Io<'a> {
    pub w: &'a mut World,
}

impl EngineIo for Io<'_> {
    fn view(&self) -> &dyn StateView {
        self.w
    }

    fn data(&self) -> &GameData {
        &self.w.s.setup.data
    }

    fn apply_damage(&mut self, d: DamageEvent) -> f64 {
        self.w.damage(d)
    }

    fn apply_heal(&mut self, h: HealEvent) {
        self.w.heal(h);
    }

    fn apply_aura(&mut self, a: AuraApplication) {
        self.w.apply_aura(a);
    }

    fn remove_aura(&mut self, holder: ActorId, aura: AuraId, source: Option<ActorId>) {
        self.w.remove_aura(holder, aura, source);
    }

    fn remove_stacks(&mut self, aura: AuraRef, stacks: u8) {
        self.w.remove_stacks(aura, stacks);
    }

    fn extend_aura(&mut self, aura: AuraRef, by: SimDuration) {
        self.w.extend_aura(aura, by);
    }

    fn add_aura_value(&mut self, aura: AuraRef, delta: f64) {
        self.w.add_aura_value(aura, delta);
    }

    fn record_proc_attempt(&mut self, listener: ListenerRef, procced: bool) {
        self.w.record_proc(listener, procced);
    }

    fn add_resource(&mut self, actor: ActorId, kind: ResourceKind, delta: f64) {
        self.w.add_resource(actor, kind, delta);
    }

    fn set_regen_mult(&mut self, actor: ActorId, kind: ResourceKind, mult: f64) {
        self.w.set_regen_mult(actor, kind, mult);
    }

    fn set_haste(&mut self, actor: ActorId, mult: f64) {
        self.w.set_haste(actor, mult);
    }

    fn set_attack_speed(&mut self, actor: ActorId, mult: f64) {
        if let Some(a) = self.w.actor_mut(actor) {
            a.attack_speed = mult;
        }
    }

    fn adjust_cooldown(&mut self, actor: ActorId, spell: SpellId, change: CooldownChange) {
        self.w.adjust_cooldown(actor, spell, change);
    }

    fn summon(
        &mut self,
        _owner: Seat,
        _pet: PetId,
        _count: u8,
        _duration: Option<SimDuration>,
    ) -> Vec<ActorId> {
        self.w.unsupported("pets");
        Vec::new()
    }

    fn dismiss(&mut self, _owner: Seat, _pet: PetId, _count: Option<u8>) {
        self.w.unsupported("pets");
    }

    fn extend_pets(&mut self, _owner: Seat, _pet: Option<PetId>, _by: SimDuration) {
        self.w.unsupported("pets");
    }

    fn command_pets(
        &mut self,
        _owner: Seat,
        _pet: Option<PetId>,
        _spell: SpellId,
        _target: ActorId,
    ) {
        self.w.unsupported("pets");
    }

    fn trigger_spell(&mut self, caster: ActorId, spell: SpellId, target: Option<ActorId>) {
        let now = self.w.now;
        self.w.queue.push(
            now,
            Event::Triggered {
                caster,
                spell,
                target,
            },
        );
    }

    fn interrupt(&mut self, _target: ActorId) -> bool {
        false
    }

    fn schedule(&mut self, delay: SimDuration, timer: TimerEvent) {
        self.w.schedule_timer(delay, timer);
    }

    fn roll(&mut self, actor: ActorId, stream: StreamKey) -> f64 {
        self.w.roll(actor, stream)
    }

    fn wake(&mut self, seat: Seat, reason: WakeReason) {
        if self.w.seat_ref(seat).is_some() {
            let now = self.w.now;
            self.w.notify(seat, reason, false, None, now);
        }
    }
}
