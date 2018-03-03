namespace policygen {
// Everything needed to keep track of *CURRENT* player state for the
// instantiated player. That means variables in here probably should NOT be
// `const`. Heuristic: if you want to reference a non-time-varying value, it
// should probably be in static storage as a const-variable in the class module.
// Examples of this sort of time-invariant thing might min power, max power,
// base GCD, base power regen, etc.

// All actor state is changed through the RESOLUTION of EVENTs, and *ONLY*
// through the resolution of events. This is important. We will batch up event
// resolution so that all events that happen in a time slice are resolved
// together. This means we either need to make sure that event resolution does
// not care about events happening in that time slice (treated as simultaneous)
// or be careful about event resolution order. This is an speed vs. accuracy
// tradeoff: we can make the timing slice tighter and tighter (we will start at
// 32 windows per second, for a timing window of 31.25 milliseconds per slice)
// to improve accuracy, but this will slow down simulation.

class Actor {
 public:
 private:
  // Actors have max health, and current health.
  int maximum_health_;
  int current_health_;

  // Actors have auras. Auras are buffs or debuffs. Auras have a duration that
  // is either finite or infinite. Auras have stacks.
  std::vector<Aura> active_auras_;

  // Actors have a `target` for all actions, but they don't own it, and
  // shouldn't be able to mutate it. It is nonsensical for an actor to have a
  // null target, but we cannot rebind references, so we use a pointer and
  // explicitly require that any events which change this piece of state do not
  // set current_target_ to NULLPTR.  The lifetime of the target must exceed or
  // equal that of the actor.  We assume that the player cannot modify their
  // target directly, but we can use this reference to get debuffs on the
  // target.
  const Actor* current_target_;
};

class Player : Actor {
 public:
 private:
  // Players have character stats:
  CharacterStats character_stats_;

  // Players have cooldown status
  std::vector<SpellCooldown> cooldowns_;

  // Players have a special global cooldown.
  SpellCooldown gcd_;

  // Players have resource (power) - energy, mana, focus, fury, rage, runes,
  // maelstrom, astral power, etc. The idea is to pick `power` as a type with
  // ~continuous behavior, hence why runes are here and runic power is below.
  ResourceType power_;

  // Some players may have "alternate power" (mana, chi, combo_points,
  // runic_power, insanity, holy_power, soul_shards etc). Mana might be listed
  // here because some classes have mana as a resource but don't spend it as
  // their primary (spriests, warlocks, non-arcane mages, balance druids)
  ResourceType alternate_power_ = nullptr;

  // Players also have talents, but because those are fixed throughout
  // simulation, we don't persist them here.

  // Players can see all available targets to hit at all time.
  const std::vector<Enemy*> available_enemies_;
};

class Enemy : Actor {
 public:
 private:
  // Enemies inherit maximum_health, current_health, auras, and their
  // current_target from Actor.
};
}  // namespace policygen
