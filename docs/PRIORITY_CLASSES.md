# Priority Classes: the Simultaneity Tie-Break Table

Event ordering key is `(time, priority, seq)` (invariant 4). At equal
timestamps, events resolve in the class order below; within a class, in
schedule order (`seq`). These are **gameplay-semantic decisions**, defined
once in `sim_engine::priority::Priority` and never bypassed. Any change to
this table is a semantics change and must be logged here with its rationale.

| # | Class | Members | Rationale |
|---|-------|---------|-----------|
| 0 | `AuraExpiry` | aura expirations | A buff ending exactly when something else resolves does **not** snapshot into it; expiry wins all same-instant races. |
| 1 | `PeriodicTick` | DoT/HoT ticks | An aura expiring exactly at tick time grants no bonus final tick (expiry at class 0 has already invalidated the train). |
| 2 | `CastComplete` | cast completions + effect application | Damage/aura application resolves before any same-instant legality wakes, so decisions see the completed state. |
| 3 | `LegalityChange` | GCD end, cooldown ready, resource crossings, combat start | Mask-changing transitions wake actors only after the gameplay state at that instant is fully resolved. |
| 4 | `Encounter` | scripted intake, displacement demands | Scripted-world effects land after player-driven resolution at the same instant. |
| 5 | `ScheduledWake` | `Wait::Until`, predicate wakes | Policy-requested wakes observe everything else that happened at that instant. |
| 6 | `Bookkeeping` | combat timeout | A kill and a timeout at the same instant is a kill. |

Review obligation: every new event type must be assigned a class here, in the
same change that introduces it.
