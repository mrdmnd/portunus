// Base class for both player characters, pets, and enemy targets.
// This class holds current state. Variables here should generally not be const.
class Actor {
 public:
  // Should not be able to actually create an actor.
  // When a derived class is constructed, the base class's constructor is called
  // as well. This allows for initialization of the base class's member
  // variables.
  virtual Actor() = 0;
  virtual ~Actor() = 0;
  virtual Actor(const Actor& other) = delete;
  virtual Actor(Actor&& other) = delete;
  virtual operator=(const Actor& other) = delete;
  virtual operator=(Actor&& other) = delete;

 private:
  // Actors have max health, and current health.
  // We don't actually estimate enemy health directly; rather, we use the health
  // estimator passed in from the simulation encounter config.
  int maximum_health_;
  int current_health_;

  // Actors have auras. Auras are buffs or debuffs. Auras have a duration that
  // is either finite or infinite. Auras have stacks.
  std::vector<Aura> active_auras_;

  // Actors have a `target` for all actions, but they don't own it, and
  // shouldn't be able to mutate it. It is nonsensical for an actor to have a
  // null target, but we cannot rebind references, so we use a pointer and
  // explicitly require that any events which change this piece of state do not
  // set current_target_ to NULLPTR.
  //
  // The lifetime of the target must exceed or equal that of the actor. We
  // assume that the player cannot modify their target directly, but we can use
  // this reference to get debuffs on the target.
  const Actor* current_target_;
};
