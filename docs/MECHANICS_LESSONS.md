# What the Mechanics Roadmap Taught Us

A look back at Phases 0 through 9 of `MECHANICS_PLAN.md` (October 2026).
The plan says what was built. This file records what we found while
building it: where the plan was wrong, what the engine turned out to need,
which shortcuts we knowingly took, and how to work on this codebase
efficiently. Read it before writing the next plan, or before starting
any of the remaining phases (10, 11, the Tier B follow-ups, and Tier C).

---

## 1. The biggest lesson: the plan's game rules were often wrong

Roughly one item in three needed a correction once checked against SimC
(`target/simc`) or current game rules. The plan was written from memory
of how the game works, and memory was frequently out of date or simply
mistaken. Every one of these would have shipped wrong without the "verify
in SimC first" rule.

| Item | The plan said | What is actually true |
|---|---|---|
| 1.1 Channels | Channel ticks are direct damage | They are periodic (`DMG_OVER_TIME`), so armor doesn't apply |
| 1.1 Channels | Swings pause during a channel | Swings that come due are *wasted* (`interrupt_auto_attack`), keeping the rhythm |
| 1.1 Channels | (undecided) when the cast "succeeds" | At the start: costs, cooldown and `CastComplete` listeners all fire then |
| 1.2 Empowers | Moving cancels the empower | Movement releases it at the stage reached, like `StopCast` (`dot_t::cancel` runs `last_tick`) |
| 1.3 Potions | Maybe the cooldown starts after combat | No: an ordinary shared category cooldown, started on use |
| 2.2 Absorbs | Absorbed damage counts as damage done | It doesn't; SimC records the attacker's damage after absorbs |
| 2.2 Absorbs | Oldest shield first | Smallest first, ties by name |
| 3.2 Execute | `TargetHpBelow` (strict) | Inclusive at the threshold (`<=`) |
| 6.1 Cheat death | Cauterize and Cheat Death as models | SimC implements neither; Ardent Defender is its only death save |
| 7.1 Army of the Dead | First ghoul at the cast | First ghoul one step *after* the cast |
| 7.2 Wild Imps | Leave at zero energy | Leave when they can't afford another bolt |
| 8.3 Stun DR | 100 → 50 → 25 → immune, 18 s reset | Since 12.1.0: halving to 1/16, never immune, 20 s reset after the stun *ends* |
| 8.4 Dispels | Newest aura first | Random pick |
| 9.3 Augmentation | SimC credits allied damage to the Evoker | It doesn't; the buffs just raise allies' DPS |
| 10 Avoidance | Enemy hits need an `aoe` flag | SimC infers area damage from the target (`is_aoe`); so do we |
| 10 Highest secondary | A modifier evaluated continuously | Picked at trigger time (`util::highest_stat`), ties to Versatility, Mastery, Haste, Crit in that order |
| 10 Armor penetration | Subtracts from armor | Multiplies armor (`a *= 1 - v`) |
| 10 Rating curves | (our data) knees at 39/47/54/66% | SimC's secondary knees are 30/40/50/60/80%; ours were wrong |
| 11 Battle res | (verify) | SimC has none; Mythic+ is 1 charge + 1 per 10 min, max 5, spent on accept |

**Takeaways:**
- Treat every game rule in a plan as a hypothesis. Find the SimC code
  path *before* designing the data shape, because the correct rule often
  changes the shape (e.g. wasted swings needed `CastKind::Channel.swings`;
  Wild Imps needed a cost check, not a zero check).
- Patch versions matter. SimC branches on `wowv()` in places (stun DR,
  Ebon Might's value); check which branch is current.
- When SimC has no model for something (cheat death, out-of-combat regen),
  say so in the doc comment and pick the simplest rule.

## 2. Engine discoveries: things that were missing or broken

These weren't on the plan at all; they surfaced while implementing
nearby items.

- **The original audit was wrong about what worked.** Channels, empowers
  and cooldown categories had data types and decision-interface hooks
  but were refused at setup. "The types exist" is not evidence that a
  feature works; look for `unsupported` calls and empty stubs.
- **Damage banks were never implemented** (Ignite, Deep Wounds, Stagger).
  They filled but nothing drew from them, silently. Added as 2.5. The
  general lesson: a data variant can exist, validate and load while the
  runtime ignores it. When touching a subsystem, grep for each variant's
  uses.
- **Latent bugs found on the way:**
  - `Coefficient::WeaponDamage` always used the main hand (1.4).
  - `DiffersFromLastCast` compared a cast with itself during its own
    effects, so it was always false (3.1).
  - The kits' hook walk skipped `Chance.then`, so hooks only reachable
    through a `Chance` were never checked (4.3).
  - Seed of Corruption's counter would have filled a separate copy
    sourced from the enemy, because listeners run as the holder (2.x;
    fixed by letting an aura's own effects address its own instance).
  - Enemy lookups by `(combat, spawn)` could misresolve enemies from past
    combats once spawns were per-combat (4.1; fixed by keeping every
    combat's spawn list).
- **Death removes auras before mechanics hears about it.** Anything that
  asks "what did the enemy have when it died" needs the snapshot
  (`StateView::auras_at_death`), and reordering death to avoid that would
  shift RNG draws.
- **Kernel-pushed values go stale.** Haste and max health are pushed to
  the kernel; everything else is computed on demand. Any new way for a
  stat to change (a Stamina buff, a stat share whose *source* changes)
  must re-push. 9.3 needed propagation from a share's source to its
  holders for exactly this reason.
- **Effect targets drop the dead.** The interpreter's `targets` filters
  out dead actors for every effect, so resurrection needed its own
  unfiltered list (Phase 11).
- **Locked seats are armed automatically.** A seat put back into
  `Locked` gets asked as soon as an ability becomes ready; an explicit
  wake on top of that was redundant (found when a mutation survived).
- **Dead seats keep their target.** `enemy_gone` and `enemy_available`
  retarget every seat, dead or alive, so a revived seat needs no target
  pickup of its own.
- **A seat with no legal non-hostile spell gets no pre-pull decisions.**
  Deliberate existing behaviour, but it surprises tests (the out-of-combat
  test gives the seat a free self-cast for this reason).

## 3. Design constraints that shaped many decisions

- **`Eq` derives block `f64`.** `ActorView`, `ListenFor`, `EffectTarget`
  and others derive `Eq`, so new fields there must be integers or live
  elsewhere: `HealthBelow { pct }` is a whole percent, `NearTarget
  { yards }` is a `u8`, pack positions live behind
  `StateView::pack_position` rather than on `ActorView`.
- **Hash stability is the safety net.** Appending enum variants at the
  end, adding `#[serde(default)]` fields, and only settling state that
  has the new behaviour (e.g. out-of-combat regen only settles resources
  that have a rate) kept every existing DPS and trace hash unchanged
  through nine phases. When an implementation would reorder events or
  draws, find another way rather than accept a hash move.
- **Random draws keep their place.** An `unmodified` copy still draws its
  crit roll so the stream doesn't shift; new randomness gets its own
  stream key.
- **Drain followups after kernel-side changes.** Any kernel path that can
  cause removals or mechanics callbacks outside a mechanics call (stops,
  movement, releases) must call `drain()`. Draining an empty queue is
  free, so adding drains didn't change traces.
- **Data first worked.** Almost every feature landed as vocabulary in
  `gamedata` plus generic interpreter support. Storm, Earth, and Fire,
  Sigils, Prescience, Bloodlust-as-a-spell and Blade Flurry needed no
  engine work at all once the building blocks existed.

## 4. Known divergences from SimC (accepted, documented in code)

Each of these is small and documented where it lives. They are the first
places to look if a spec's numbers drift from SimC.

- **Blade Flurry copies don't proc poisons** (nothing listens to a copy,
  because of the depth guard). SimC's do.
- **Blade Flurry copies use the landed amount with `ignores_armor`**
  instead of re-applying each secondary target's armor. Equal when the
  pack shares armor.
- **Banks recompute their draw each tick.** Same as SimC unless haste
  changes the tick count mid-bank, in which case ours still empties the
  bank and SimC's doesn't.
- **Bank ticks run the normal damage pipeline.** SimC's residuals can't
  crit and snapshot multipliers; revisit when Fire Mage or Arms needs it.
- **No mana drain per channel tick** (`cost_per_tick`).
- **Dispels don't refuse a cast when there is nothing to dispel.**
- **Haste-conditioned modifiers don't reschedule timers** when the
  condition flips mid-walk or on a health crossing.
- **Allies are observed with no reaction delay.**
- **Leech heals each hit at once**; SimC pools it and heals every hasted
  GCD.
- **Speed follows SimC's formula** (× `exp(-0.0003 × rating)` × 0.1),
  which looks suspicious but is copied as is.
- **A wipe ends the pull** even if a Soulstone is waiting for a charge.
- **No damage credit for Augmentation** (matches SimC, but the game's
  logs do attribute some damage to the Evoker).

## 5. Testing lessons

- **Mutation testing earned its keep.** After each feature, breaking each
  piece in turn and checking a test fails found a weak test about once
  per phase. Typical causes, all fixed:
  - the test data made the branch unreachable (dummy with no armor; 100
    energy at 20 per bolt landing exactly on zero; no enemy between 8 and
    9 yards; nothing the seat applies being dispellable);
  - the ordering hid the behaviour (a pre-infected add placed after the
    spread cap was reached);
  - a duration ran out before the effect mattered (Bloodlust expiring
    before the boss died);
  - no time passed between two events that differ only in timing (rune
    refills).
  When a mutation survives, either fix the test data or find that the
  code was redundant (the "target always counts" clause in `NearTarget`
  was, and was removed).
- **The synthetic fixture pays off.** One shared `tests/synthetic` fixture
  (real game data, classes cleared, crits off, a boss that can't die)
  made most semantic tests short. It now supports multiple seats.
- **Pitfalls to remember** (also in plan §1.6): class raid buffs leak into
  fixtures; pull health varies ±20% unless pinned; pre-pull decisions come
  before combat starts, so scripts that should act in combat must check
  `combat_time`; passive auras show up in aura lists. `play_party` walks
  one script for all seats, so when the next step is one seat's and both
  wait for the next event, nothing may come and the step never runs;
  give each seat its own queue instead (as `arms_engine.rs` does). A hit
  of zero damage leaves no damage event in the trace.
- **More trace pitfalls, from Arms.** The trace reports `AuraApplied`
  when a stack is taken away too (2 → 1), so count procs as events
  where the stack count doesn't fall. Enemies don't exist before the
  pull spawns them. A seat that waits for the next event may not be
  woken by its own swings, so use `Wait::Until` to watch something
  closely. Stopping at a decision without submitting a choice and then
  advancing again returns the same decision forever.
- **Mutate every copy.** A data mutation must change every listener or
  modifier it means to break: `perl -0pi -e 's/.../'` without `/g`
  changes only the first match. That match may even be in another
  spec's data, and then the mutation tests nothing.
- The test count grew from 117 early in the work to 230 after Phase 9,
  241 after Phase 11, 255 after the Arms engine additions, and 271 with
  core Arms.

## 6. Process lessons (tooling and workflow)

- **Run the four gates together** with one command and summarise test
  results with `awk`; it keeps the loop fast and the output readable.
- **`cargo fmt` reformats code**, so a scripted text replacement written
  before formatting may not match after. Run fmt, then edit; or edit by
  line range.
- **Scripted multi-file edits must not reuse match offsets** after
  inserting text. One crashed script did this and had to be audited.
- **`xargs` fails in the sandbox.** Use `$(rg -l ...)` instead.
- **Struct-literal fallout is predictable.** Each new field breaks the
  same handful of literal sites; plan §1.5 step 2 lists the searches.
- **Pushing to master needs explicit approval** each time, and the
  approval prompt can fail; check `git status -sb` for "ahead" after
  every push.
- **Phase-sized commits worked well**: each commit message lists what was
  added and what is still refused, and the plan's "Done" notes double as
  a changelog.

## 7. Recommendations for the next plan

1. **The engine is ahead of the data.** Almost none of Phases 1–9 is used
   by checked-in data; that is why no DPS changed. The vocabulary is only
   proven by synthetic tests. The most valuable next step is the
   per-spec data track (plan §15): pick one spec that exercises many new
   features (Affliction Warlock, Outlaw Rogue or Augmentation Evoker),
   write its data, kit, loadout and script, and compare against SimC.
   Expect that to surface more corrections like those in section 1.
2. **Verify first, design second.** Make "find the SimC code path" the
   first step of each item, before the data design, and write the
   finding into the plan item before implementing.
3. **Keep the "Change from the plan" convention.** It made the plan
   trustworthy as a record; future phases should keep marking
   corrections inline.
4. **Revisit the accepted divergences** (section 4) when the spec that
   needs them arrives, not before.
5. **Remaining roadmap work:** Phase 10 (stats and math), Phase 11
   (survival), the Tier B follow-ups (1.5–1.8, 6.2, 6.3), and the deferred
   9.3 damage credit. Tier C still needs the user's go-ahead.
