# Arms policy tuning

Hand-driven hill climbing on the Arms policies in
`crates/run/src/scripts/arms.rs`. Each candidate is the best so far plus
one change, expressed as a `Tweaks` field; `arms` (all off) is the port
of SimC's Slayer lists.

Method: `portunus-run compare data/runs/arms_long.ron --policy CAND
--baseline BEST`. Tuning seeds are 0–199 (0–599 to settle close calls);
the final check runs on the held-out seeds (`--holdout`, from 1,000,000).
Intervals are 95% on the paired per-seed DPS difference.

## With Slayer (current)

`arms` now runs `slayer_execute` in the execute phase and spends Sudden
Death after Mortal Strike while Executioner is up: SimC's own Slayer
lines, and the old round's h1 and h2. On `arms_long` it does 85.5k.

Round one, each against `arms`:

| candidate | change | result |
|---|---|---|
| `arms_bladestorm_cd` | Bladestorm as soon as it's ready | +0.49% [+0.24, +0.74] (600 seeds); holdout +0.25% [−0.00, +0.49] |
| `arms_execute_free_first` | execute: free Execute right after Colossus Smash | +0.06%, unclear |
| `arms_ungated` | Sudden Death after Mortal Strike without Executioner | −0.05%, unclear |
| `arms_execute_early` | Sudden Death ahead of Mortal Strike | −0.14%, unclear |
| `arms_execute_rage20` | execute: Execute ahead of Overpower above 20 rage | −0.15%, unclear |
| `arms_ms_precision1` | execute: Mortal Strike at 1 Executioner's Precision | −0.40%, worse |
| `arms_op_first` | Overpower at 2 charges ahead of Mortal Strike | −0.66%, unclear |
| `arms_avatar_cd` | Avatar on cooldown | −0.70%, worse |
| `arms_rend_last` | Cleave for Rend only in its last GCD | −0.86%, worse |
| `arms_no_demise_execute` | no Execute for Imminent Demise before Bladestorm | −0.86%, worse |
| `arms_op_capped` | execute: Overpower at 2 charges ahead of Execute (old h3) | −1.17%, worse |
| `arms_demise3` | Bladestorm only at 3 Imminent Demise stacks | −3.02%, worse |
| `arms_smash_cd` | Colossus Smash ignoring Sudden Death stacks | exactly 0, as before |

Best so far: `arms_bladestorm_cd`, by the method above, but only just.
The port is close to a local optimum for these changes. Bladestorm on
cooldown is the one gain on the tuning seeds, but on the held-out seeds
its interval reaches zero, and on `arms_dummy` it's identical (that
fight ends before a second Bladestorm). The old h3 now loses: with
Slayer's Malice and Executioner, Execute outweighs a capped Overpower.
`arms_ungated` barely differs because Executioner is almost always up.

## Before Slayer (b7d450e)

The round below ran on the spec tree alone (`arms.ron`, no hero tree),
with `arms` as `slayer_st` read without Slayer. Its candidates were
removed with the Slayer port; commit b7d450e has them.

### Best: `arms_h4`

| step | change over the previous step | vs previous |
|---|---|---|
| `arms_h1` | run `slayer_execute` in the execute phase | +6.49% [+6.06, +6.93] |
| `arms_h2` | outside execute, spend Sudden Death just ahead of Overpower | +4.40% [+3.85, +4.94] |
| `arms_h3` | in execute, Overpower at 2 charges ahead of Execute | +0.38% [+0.24, +0.51] (600 seeds) |
| `arms_h4` | Bladestorm as soon as it's ready, not aligned to Colossus Smash or Avatar | +0.64% [+0.35, +0.93] (600 seeds) |

Holdout, `arms_h4` vs `arms`, 600 seeds:
- `arms_long`: +11.81% [+11.42, +12.19]
- `arms_dummy` (not used for tuning): +12.72% [+12.05, +13.39]

### Why the winners win

- **h1:** the port ran `slayer_st` all fight, so in the execute phase it
  kept Slamming. The execute list cut Slams from 33 to 20 casts per run
  and raised paid Executes from 3 to 13.
- **h2:** `slayer_st` only spends Sudden Death at 2 stacks or while
  Bladestorm is ready, so procs were overwritten.
- **h4:** Bladestorm itself loses about 290 DPS, but Mortal Strike and
  Overpower gain more. The Bladestorm channel no longer takes up the
  Colossus Smash window.

### Rejected

Results are against the step named in the "base" column. "Unclear" means
the interval includes zero.

| candidate | change | base | result |
|---|---|---|---|
| `arms_h2a` | Sudden Death ahead of Mortal Strike instead | h1 | +4.27%, tied with h2 (h5 below) |
| `arms_h3a` | execute: Mortal Strike at 1 Executioner's Precision stack | h1 | −0.39%, worse |
| `arms_h3b` | execute: Mortal Strike whenever ready | h1 | −0.39%, worse |
| `arms_h4` (old) | execute: Execute ahead of Overpower only above 60 rage | h1 | +0.20% [−0.07, +0.47], unclear |
| `arms_h5` | Sudden Death ahead of Mortal Strike | h2 | −0.12%, unclear |
| `arms_h6` | Sudden Death ahead of Heroic Strike | h2 | −0.33%, unclear |
| `arms_h7` | execute: free Execute right after Colossus Smash | h2 | −0.14%, unclear |
| `arms_h8a` | Slam only above 50 rage | h2 | −0.48%, unclear |
| `arms_h8b` | Slam only above 70 rage | h2 | −1.08%, worse |
| `arms_h9a` | no Slam from 40% health until execute | h2 | −0.25%, unclear |
| `arms_h9b` | no Slam from 45% health until execute | h2 | −0.67%, worse |
| `arms_h10a` | Overpower ahead of Mortal Strike at 2 charges | h2 | +0.09%, unclear |
| `arms_h10b` | Overpower always ahead of Mortal Strike | h2 | +0.06%, unclear |
| `arms_h12` | Avatar on cooldown | h2 | −0.40%, worse |
| `arms_h14` | Colossus Smash ignoring Sudden Death stacks | h2 | exactly 0 (see below) |
| `arms_h15a` | Cleave for Rend only in its last GCD | h2 | −0.84%, worse |
| `arms_h15b` | never Cleave for Rend | h2 | −5.07%, worse |

`arms_h14` is identical by construction. Colossus Smash's
`buff.sudden_death.stack<2` can only fail at 2 stacks, and at 2 stacks
the Execute line just above it always fires first.

Only the h1–h4 steps and the round-three rejects (h10–h15) are still
registered under these names. The earlier rejects can be rebuilt from
their `Tweaks` field: `free_execute`, `execute_ms_precision`,
`execute_rage`, `free_execute_first`, `slam_rage` and `pool_below`. The
old `arms_h4` (the 60-rage threshold) predates the current `arms_h4`
name.
