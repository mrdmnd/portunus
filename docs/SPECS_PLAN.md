# All DPS Specializations & Hero Trees Plan

Master implementation plan for bringing all **27 DPS specializations** and
**35 DPS Hero Talent trees** (World of Warcraft 12.1 / SimulationCraft
`MID2` & `MID1` reference profiles) into Portunus at full SimC reference-build
fidelity.

---

## 1. Ground Rules & Per-Spec Definition of Done

Every spec and hero tree follows the standard established by Elemental Shaman
(`crates/mechanics/tests/{elemental,stormbringer,farseer}.rs`) and Arms
Warrior (`docs/ARMS_PLAN.md`, `crates/mechanics/tests/arms.rs`):

1. **SimC verification first:** Before authoring RON data or APLs, inspect
   - `target/simc/engine/class_modules/sc_<class>.cpp` (or subdir),
   - `target/simc/SpellDataDump/<class>.txt`,
   - `target/simc/engine/dbc/generated/trait_data.inc`, and
   - `target/simc/profiles/MID2/` (falling back to `MID1` where `MID2` has
     only one hero tree).
2. **Engine before data:** If a spec or hero tree needs a mechanic that the
   effect/modifier vocabulary in `crates/gamedata` cannot express cleanly,
   add the general vocabulary item first (following `docs/MECHANICS_PLAN.md`
   §1.5) or implement a `SpecKit` hook in `crates/mechanics/src/kits.rs`.
3. **Data (`data/game.ron`):**
   - `SpecDef` with primary stat, base mastery, resources, baseline spells
     (including damage-relevant class-tree actives/passives from the
     reference build), and the 12.1 spec talent tree from `trait_data.inc`.
   - `HeroTreeDef` for each of the spec's two hero trees (14 points, free
     keystone).
   - `SpellDef`, `AuraDef`, and `PetDef` entries with coefficients, proc
     rates, and durations taken from `SpellDataDump/<class>.txt` and
     `sc_<class>.cpp`.
4. **Loadouts (`data/loadouts/`):** One compiled reference loadout per
   `(spec, hero_tree)` pair (e.g., `arms_slayer.ron`, `arms_colossus.ron`,
   `fury_slayer.ron`, `fury_mountain_thane.ron`).
5. **Policies (`crates/run/src/scripts/`):** Port of SimC's single-target,
   execute, and multi-target/AoE action priority lists for both hero trees,
   registered in `SCRIPTS`.
6. **Run files (`data/runs/`):** At least one single-target dummy run and
   multi-target/long run where applicable.
7. **Tests (`crates/mechanics/tests/` & `crates/ingest/tests/data_files.rs`):**
   - Loadout compilation and `check_game_data` validation.
   - Unit and rollout tests verifying core rotational mechanics, resource
     economy, mastery, cooldown interactions, and each hero tree's keystone
     and capstones.
8. **Four gates:** `cargo fmt --all`, `cargo clippy --workspace --all-targets
   -- -D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace
   --no-deps`, and `cargo test --workspace` must all pass with zero warnings.

---

## 2. Master Inventory: 27 DPS Specs & 35 Hero Trees

| Phase | Class | Spec (`SpecId`) | Hero Tree 1 (`HeroTreeId`) | Hero Tree 2 (`HeroTreeId`) | Status |
|---|---|---|---|---|---|
| **P1** | Shaman | Elemental (`262`) | Stormbringer (`54`) | Farseer (`55`) | **Done** |
| **P1** | Warrior | Arms (`71`) | Slayer (`60`) | Colossus (`62`) | **Done** |
| **P1** | Warrior | Fury (`72`) | Slayer (`60`) | Mountain Thane (`61`) | **Done** |
| **P2** | Shaman | Enhancement (`263`) | Stormbringer (`54`) | Totemic (`56`) | **Done** |
| **P3** | Paladin | Retribution (`70`) | Templar (`48`) | Herald of the Sun (`50`) | **Done** |
| **P4** | Death Knight | Frost (`251`) | Deathbringer (`33`) | Rider of the Apocalypse (`32`) | **Done** |
| **P4** | Death Knight | Unholy (`252`) | San'layn (`31`) | Rider of the Apocalypse (`32`) | **Done** |
| **P5** | Hunter | Beast Mastery (`253`) | Pack Leader (`43`) | Dark Ranger (`44`) | **Done** |
| **P5** | Hunter | Marksmanship (`254`) | Sentinel (`42`) | Dark Ranger (`44`) | **Done** |
| **P5** | Hunter | Survival (`255`) | Pack Leader (`43`) | Sentinel (`42`) | **Done** |
| **P6** | Rogue | Assassination (`259`) | Deathstalker (`53`) | Fatebound (`52`) | **Done** |
| **P6** | Rogue | Outlaw (`260`) | Trickster (`51`) | Fatebound (`52`) | **Done** |
| **P6** | Rogue | Subtlety (`261`) | Deathstalker (`53`) | Trickster (`51`) | **Done** |
| **P7** | Mage | Arcane (`62`) | Sunfury (`39`) | Spellslinger (`40`) | Planned |
| **P7** | Mage | Fire (`63`) | Sunfury (`39`) | Frostfire (`41`) | Planned |
| **P7** | Mage | Frost (`64`) | Spellslinger (`40`) | Frostfire (`41`) | Planned |
| **P8** | Warlock | Affliction (`265`) | Hellcaller (`58`) | Soul Harvester (`57`) | Planned |
| **P8** | Warlock | Demonology (`266`) | Diabolist (`59`) | Soul Harvester (`57`) | Planned |
| **P8** | Warlock | Destruction (`267`) | Diabolist (`59`) | Hellcaller (`58`) | Planned |
| **P9** | Priest | Shadow (`258`) | Archon (`19`) | Voidweaver (`18`) | Planned |
| **P10** | Monk | Windwalker (`269`) | Conduit of the Celestials (`64`) | Shado-Pan (`65`) | Planned |
| **P11** | Druid | Balance (`102`) | Elune's Chosen (`24`) | Keeper of the Grove (`23`) | Planned |
| **P11** | Druid | Feral (`103`) | Druid of the Claw (`21`) | Wildstalker (`22`) | Planned |
| **P12** | Demon Hunter | Havoc (`577`) | Aldrachi Reaver (`35`) | Fel-Scarred (`34`) | Planned |
| **P12** | Demon Hunter | Devourer (`1480`) | Void-Scarred (`126`) | Annihilator (`124`) | Planned |
| **P13** | Evoker | Devastation (`1467`) | Scalecommander (`36`) | Flameshaper (`37`) | Planned |
| **P13** | Evoker | Augmentation (`1473`) | Scalecommander (`36`) | Chronowarden (`38`) | Planned |

---

## 3. Phase Breakdown

### Phase 1: Warrior Completion — Arms (Colossus & Multi-Target) + Fury (Slayer & Mountain Thane)
- **Arms Phase D (Multi-Target):**
  - *Sweeping Strikes* (`260708`): 15 s buff granting `ChainTargetsAdd(1.0)`
    and `ChainSharePct(75.0)` to single-target melee abilities (*Mortal
    Strike*, *Execute*, *Overpower*, *Slam* / *Heroic Strike*).
  - *Cleave* (`845`) with *Blood and Thunder*: spreads *Rend* (`388539`) to
    up to 4 other enemies via `SpreadAura { refresh: true }` and rolls
    *Reap the Storm* (`446005`) at 20% when `EnemiesAtLeast(3)`.
  - Policy: add `slayer_aoe` (at 3+ enemies) and 2-target *Sweeping Strikes*
    lines to `crates/run/src/scripts/arms.rs`.
- **Arms Colossus (`HeroTreeId(62)`):**
  - *Demolish* (`436358`): 2 s hasted 3-tick channel (ticks 1–2 deal AoE
    damage; final tick deals massive primary + AoE damage), consuming all
    *Colossal Might* (`440989`, up to 10 stacks gained from *Mortal Strike*
    and *Cleave*) via `ConsumeStacks` to scale *Demolish* by +10% per stack.
  - *Dominance of the Colossus* (`440993`), *Martial Expert* (`440990`),
    *Colossal Might*, *Earthquaker*, *Arterial Bleed*, *Tide of Battle*,
    *No Stranger to Pain*, *Practiced Strikes*, *Mountain of Muscle and
    Scars*, *Crashing Force*, *One Against Many*.
  - Loadout `data/loadouts/arms_colossus.ron` and policy lists
    `colossus_st`, `colossus_execute`, and `colossus_aoe` in `arms.rs`.
- **Fury Warrior (`SpecId(72)`):**
  - Dual-wield two-handers (*Titan's Grip*), *Enrage* (`184362`, haste +
    mastery-scaled damage *Unshackled Fury* via `MasteryDamagePct` +
    `HastePct`), *Rampage* (`184367`, 80 rage, 4 strikes across both hands,
    triggers *Enrage*), *Bloodthirst* (`23881`, 30% chance to *Enrage*,
    upgraded to *Bloodbath* `335096` during *Recklessness*), *Raging Blow*
    (`85288`, 2 charges, both-hand strike, 20% reset chance, upgraded to
    *Crushing Blow* `335097` during *Recklessness*), *Execute* (`5308` /
    *Massacre* `206315`, *Sudden Death*, *Ashen Juggernaut*), *Whirlwind*
    (`190411`, *Improved Whirlwind* / *Meat Cleaver* `85739` 4 stacks causing
    single-target abilities to hit 4 extra targets for 55% via
    `ChainTargetsAdd` / `ChainSharePct`), *Recklessness* (`1719`, +20% crit,
    +100% rage generation), *Avatar* (`107574`), *Odyn's Fury* (`385059`),
    *Thunder Clap* (`6343`), *Bladestorm* (`46924`).
  - **Fury Slayer (`HeroTreeId(60)`):** Shares tree `60` with Arms (*Slayer's
    Dominance*, *Executioner*, *Imminent Demise*, *Overwhelming Blades*,
    *Reap the Storm*, *Brutal Finish*, *Fierce Followthrough*, *Opportunist*,
    *Unhinged*, *Unrelenting Onslaught*).
  - **Fury Mountain Thane (`HeroTreeId(61)`):** *Lightning Strikes*
    (`434969`), *Thunder Blast* (`435222` / `435607`), *Crashing Thunder*,
    *Ground Current*, *Strength of the Mountain*, *Burst of Power*,
    *Flashing Skies*, *Thorim's Might*, *Keep Your Feet on the Ground*,
    *Avatar of the Storm*.
  - Loadouts `data/loadouts/fury_slayer.ron` and
    `data/loadouts/fury_mountain_thane.ron`, run file
    `data/runs/fury_dummy.ron`, script `crates/run/src/scripts/fury.rs`,
    and test suite `crates/mechanics/tests/fury.rs`.

### Phase 2: Enhancement Shaman (`SpecId(263)`)
- Dual-wield melee, *Maelstrom Weapon* (10 stacks, spent up to 5 or 10 via
  `ConsumeStacks`), *Windfury Weapon* (`ExtraSwing`), *Flametongue Weapon*,
  *Stormstrike* / *Windstrike* (both-hand strikes), *Lava Lash* (*Hot Hand*),
  *Ice Strike*, *Crash Lightning*, *Sundering*, *Feral Spirit*, *Doom Winds*,
  *Ascendance*.
- Hero trees: **Stormbringer (`54`)** (shared with Elemental, *Tempest*
  consuming *Maelstrom Weapon*) and **Totemic (`56`)** (*Surging Totem*,
  *Lively Totems*, *Whirling Elements*, *Oversurge*).

### Phase 3: Retribution Paladin (`SpecId(70)`)
- `ResourceKind::HolyPower` (5 max), *Blade of Justice*, *Judgment*,
  *Hammer of Wrath*, *Templar's Verdict* / *Final Verdict*, *Divine Storm*,
  *Wake of Ashes* (grants Holy Power + *Truth's Wake*), *Avenging Wrath* /
  *Crusade*, *Execution Sentence*, *Final Reckoning*, *Divine Toll*,
  *Consecration* (ground aura).
- Hero trees: **Templar (`48`)** (*Lights Guidance* / *Hammer of Light* after
  *Wake of Ashes*, *Empyrean Hammer*) and **Herald of the Sun (`50`)**
  (*Dawnlight*, *Sun's Avatar*, *Gleaming Rays*).

### Phase 4: Death Knight — Frost (`SpecId(251)`) & Unholy (`SpecId(252)`)
- `ResourceKind::Runes` (6 runes, 3 concurrent recharge) + `RunicPower`.
- **Frost (`251`):** *Obliterate*, *Frost Strike*, *Howling Blast* (*Rime*),
  *Killing Machine*, *Remorseless Winter*, *Pillar of Frost*, *Breath of
  Sindragosa*, *Frostwyrm's Fury*, *Glacial Advance*.
  - Hero trees: **Deathbringer (`33`)** (*Reaper's Mark*, *Exterminate*,
    *Wave of Souls*) and **Rider of the Apocalypse (`32`)** (*Apocalypse's
    Four Horsemen* guardians, *Mograine's Death and Decay*).
- **Unholy (`252`):** Permanent Ghoul pet, *Festering Strike* / *Festering
  Wound* (`ConsumeStacks` on *Scourge Strike* / *Clawing Shadows* and
  *Apocalypse*), *Virulent Plague*, *Death Coil*, *Epidemic*
  (`EnemiesWithAura`), *Dark Transformation*, *Summon Gargoyle*, *Army of
  the Dead* (`stagger`), *Death and Decay* (`InOwnGround` cleave).
  - Hero trees: **San'layn (`31`)** (*Vampiric Strike*, *Essence of the
    Blood Queen*, *Infliction of Sorrow* via `TickAura`) and **Rider of the
    Apocalypse (`32`)**.

### Phase 5: Hunter — Beast Mastery (`253`), Marksmanship (`254`), Survival (`255`)
- `ResourceKind::Focus` + ranged auto-shot (`WeaponDef.ranged = true`).
- **Beast Mastery (`253`):** Permanent pet + secondary pet, *Barbed Shot*
  (*Frenzy* stacks on pet + focus grants), *Kill Command* (`CommandPet`),
  *Cobra Shot*, *Bestial Wrath*, *Beast Cleave*, *Call of the Wild*,
  *Dire Beast*.
  - Hero trees: **Pack Leader (`43`)** (*Vicious Hunt*, *Howl of the Pack
    Leader*) and **Dark Ranger (`44`)** (*Black Arrow*, *Withering Fire*).
- **Marksmanship (`254`):** *Aimed Shot* (2 charges, cast time), *Rapid Fire*
  (channel), *Arcane Shot* / *Multi-Shot* (*Precise Shots*, *Trick Shots*
  chain targets), *Steady Shot* (*Steady Focus* via `RecentCasts`),
  *Kill Shot*, *Trueshot*, *Volley*.
  - Hero trees: **Sentinel (`42`)** (*Sentinel* Lunar Storm debuff/ticks) and
    **Dark Ranger (`44`)**.
- **Survival (`255`):** Melee + pet, *Mongoose Bite* (*Mongoose Fury* window),
  *Kill Command*, *Wildfire Bomb* (2 charges), *Flanking Strike*,
  *Explosive Shot*, *Butchery*, *Coordinated Assault*, *Spearhead*.
  - Hero trees: **Pack Leader (`43`)** and **Sentinel (`42`)**.

### Phase 6: Rogue — Assassination (`259`), Outlaw (`260`), Subtlety (`261`)
- `ResourceKind::Energy` + `ResourceKind::ComboPoints` (5–7 max), Stealth /
  Vanish / Subterfuge, Poisons (`WeaponHit`).
- **Assassination (`259`):** *Mutilate* (both hands), *Garrote*, *Rupture*
  (`per_unit_spent`), *Envenom*, *Crimson Tempest*, *Kingsbane*, *Deathmark*,
  *Indiscriminate Carnage*, *Caustic Spatter*.
  - Hero trees: **Deathstalker (`53`)** (*Deathstalker's Mark*, *Darkest
    Night*) and **Fatebound (`52`)** (*Hand of Fate* coin-flip heads/tails).
- **Outlaw (`260`):** *Sinister Strike*, *Pistol Shot* (*Opportunity*),
  *Between the Eyes*, *Dispatch*, *Blade Flurry* (damage copy), *Adrenaline
  Rush* (`CooldownRatePct`), *Roll the Bones* (`RandomOf`), *Killing Spree*,
  *Ghostly Strike*.
  - Hero trees: **Trickster (`51`)** (*Unseen Blade*, *Coup de Grace*) and
    **Fatebound (`52`)**.
- **Subtlety (`261`):** *Shadowstrike*, *Backstab* / *Gloomblade*,
  *Eviscerate*, *Secret Technique*, *Black Powder*, *Shadow Dance*,
  *Symbols of Death*, *Shadow Blades*, *Shuriken Storm*.
  - Hero trees: **Deathstalker (`53`)** and **Trickster (`51`)**.

### Phase 7: Mage — Arcane (`62`), Fire (`63`), Frost (`64`)
- `ResourceKind::Mana` (plus `ArcaneCharges` for Arcane).
- **Arcane (`62`):** *Arcane Blast*, *Arcane Barrage* (spends *Arcane
  Charges*), *Arcane Missiles* (*Clearcasting* channel), *Arcane Orb*,
  *Touch of the Magi*, *Arcane Surge*, *Evocation*.
  - Hero trees: **Sunfury (`39`)** (*Spellfire Spheres*, *Arcane Phoenix*
    guardian) and **Spellslinger (`40`)** (*Splinters*, *Splinterstorm*).
- **Fire (`63`):** *Fireball*, *Fire Blast* (`usable_while_casting`, always
  crits), *Phoenix Flames*, *Pyroblast* / *Flamestrike* (*Heating Up* →
  *Hot Streak*), *Combustion*, *Meteor* (`min_travel`), *Ignite* (rolling
  bank), *Shifting Power* (`CooldownRatePct` channel).
  - Hero trees: **Sunfury (`39`)** and **Frostfire (`41`)** (*Frostfire
    Bolt*, *Frostfire Mastery*, *Excess Frost* / *Excess Fire*).
- **Frost (`64`):** *Frostbolt*, *Ice Lance* (*Fingers of Frost*, *Winter's
  Chill* shatter crits), *Flurry* (*Brain Freeze*), *Frozen Orb*, *Blizzard*
  (ground AoE), *Glacial Spike*, *Comet Storm*, *Icy Veins*, *Ray of Frost*.
  - Hero trees: **Spellslinger (`40`)** and **Frostfire (`41`)**.

### Phase 8: Warlock — Affliction (`265`), Demonology (`266`), Destruction (`267`)
- `ResourceKind::SoulShards` + permanent demon pets.
- **Affliction (`265`):** *Agony*, *Corruption* / *Wither*, *Unstable
  Affliction* (`unique_per_source`), *Malefic Rapture* (`EnemiesWithAura`),
  *Shadow Bolt* / *Drain Soul* (*Nightfall*), *Seed of Corruption*
  (`threshold`), *Vile Taint*, *Soul Rot*, *Summon Darkglare*.
  - Hero trees: **Hellcaller (`58`)** (*Wither*, *Blackened Soul*,
    *Malevolence*) and **Soul Harvester (`57`)** (*Demonic Soul*, *Soul
    Anathema*).
- **Demonology (`266`):** *Shadow Bolt*, *Demonbolt* (*Demonic Core*),
  *Hand of Gul'dan* (summons *Wild Imps* with `expires_when_empty`),
  *Call Dreadstalkers*, *Summon Demonic Tyrant* (`ExtendPets`), *Summon
  Vilefiend*, *Implosion* (`Dismiss`).
  - Hero trees: **Diabolist (`59`)** (*Diabolic Ritual* → *Overlord* /
    *Mother of Chaos* / *Pit Lord*, *Ruination*) and **Soul Harvester (`57`)**.
- **Destruction (`267`):** *Incinerate*, *Immolate* / *Wither*, *Conflagrate*
  (*Backdraft*), *Chaos Bolt* (always crits, scales with crit chance via
  `CritChanceAddsDamage`), *Rain of Fire* (ground AoE), *Shadowburn*,
  *Channel Demonfire*, *Havoc* (damage copy), *Summon Infernal*.
  - Hero trees: **Diabolist (`59`)** and **Hellcaller (`58`)**.

### Phase 9: Shadow Priest (`SpecId(258)`)
- `ResourceKind::Insanity`, *Shadowform* / *Voidform* / *Dark Ascension*,
  *Vampiric Touch*, *Shadow Word: Pain*, *Devouring Plague* (rolling bank
  with `tick_zero`), *Mind Blast*, *Mind Flay* / *Mind Flay: Insanity*
  (channel), *Void Bolt*, *Shadow Word: Death*, *Halo*, *Void Torrent*,
  *Shadowfiend* / *Mindbender*, *Shadowy Apparitions* (projectiles),
  *Psychic Link* (direct damage copied to targets with *Vampiric Touch*).
- Hero trees: **Archon (`19`)** (*Power Surge* Halo waves, *Perfected Form*,
  *Resonant Energy*) and **Voidweaver (`18`)** (*Entropic Rift* ground/pet
  window, *Void Blast*, *Collapsing Void*).

### Phase 10: Windwalker Monk (`SpecId(269)`)
- `ResourceKind::Energy` + `ResourceKind::Chi`, *Mastery: Combo Strikes*
  (`DiffersFromLastCast` + `MasteryDamagePct`), *Tiger Palm*, *Blackout Kick*
  (*Teachings of the Monastery*), *Rising Sun Kick*, *Fists of Fury*
  (channel), *Whirling Dragon Punch*, *Strike of the Windlord*, *Spinning
  Crane Kick* (*Mark of the Crane* `EnemiesWithAura` count), *Touch of Death*
  (`TargetHpBelowCasterMaxHp`), *Storm, Earth, and Fire* (`CommandPet`
  echoing channels and strikes), *Invoke Xuen, the White Tiger*.
- Hero trees: **Conduit of the Celestials (`64`)** (*Celestial Conduit*
  channel with `swings: true`, *Courage of the White Tiger*, *Strength of
  the Black Ox*) and **Shado-Pan (`65`)** (*Flurry Strikes* energy-spent
  threshold counter, *Wisdom of the Wall*).

### Phase 11: Druid — Balance (`102`) & Feral (`103`)
- **Balance (`102`):** `ResourceKind::AstralPower`, *Moonkin Form*, *Moonfire*,
  *Sunfire*, *Stellar Flare*, *Wrath*, *Starfire*, *Eclipse (Solar / Lunar)*,
  *Starsurge*, *Starfall*, *Celestial Alignment* / *Incarnation: Chosen of
  Elune*, *Force of Nature* (Treants), *Fury of Elune*, *Full Moon*.
  - Hero trees: **Elune's Chosen (`24`)** (*Boundless Moonlight*, *Lunar
    Calling*) and **Keeper of the Grove (`23`)** (*Dream Surge*, *Harmony of
    the Grove*).
- **Feral (`103`):** `ResourceKind::Energy` + `ResourceKind::ComboPoints`,
  *Cat Form* (1.0 s GCD via `FormDef.gcd`), *Prowl*, *Rake* (stealth
  snapshot), *Shred*, *Swipe* / *Brutal Slash*, *Thrash*, *Rip*
  (`per_unit_spent`), *Ferocious Bite* (`extra` energy scaling), *Primal
  Wrath*, *Tiger's Fury* (`PersistentPct` snapshot), *Berserk* /
  *Incarnation: Avatar of Ashamane*, *Feral Frenzy*, *Convoke the Spirits*,
  *Bloodtalons*.
  - Hero trees: **Druid of the Claw (`21`)** (*Ravage* replacing *Ferocious
    Bite*, *Claw Rampage*) and **Wildstalker (`22`)** (*Thriving Growth*
    *Bloodseeker Vines* periodic debuff).

### Phase 12: Demon Hunter — Havoc (`577`) & Devourer (`1480`)
- `ResourceKind::Fury`.
- **Havoc (`577`):** *Demon's Bite* / *Demon Blades*, *Chaos Strike* /
  *Annihilation*, *Blade Dance* / *Death Sweep* (`full_primary` AoE),
  *Eye Beam* (channel with `full_primary`, grants *Metamorphosis*),
  *Immolation Aura*, *Sigil of Flame* (ground aura), *Felblade*, *Fel Rush*,
  *Vengeful Retreat* (*Initiative* / *Inertia*), *Essence Break*, *The Hunt*,
  *Metamorphosis*.
  - Hero trees: **Aldrachi Reaver (`35`)** (*Reaver's Glaive*, *Rending
    Strike*, *Glaive Flurry*, *Thrill of the Fight*) and **Fel-Scarred
    (`34`)** (*Demonsurge* explosions on *Metamorphosis* empowerments).
- **Devourer (`1480`):** Midnight 12.1 Demon Hunter DPS spec from
  `sc_demon_hunter.cpp` and `MID2_Demon_Hunter_Devourer.simc`.
  - Hero trees: **Void-Scarred (`126`)** and **Annihilator (`124`)**.

### Phase 13: Evoker — Devastation (`1467`) & Augmentation (`1473`)
- `ResourceKind::Essence` (`RechargeDef`: 5–6 max, 1 concurrent recharge,
  5 s hasted) + `ResourceKind::Mana`.
- **Devastation (`1467`):** *Fire Breath* (`CastKind::Empower` stages 1–4,
  direct + DoT inverse by stage), *Eternity Surge* (`CastKind::Empower`
  stages 1–4), *Disintegrate* (channel, *Essence Burst*, *Mass Disintegrate*),
  *Pyre*, *Living Flame* (*Burnout* instant procs), *Shattering Star*,
  *Dragonrage*, *Deep Breath*, *Hover* (`CastWhileMoving`), *Mastery:
  Giantkiller*.
  - Hero trees: **Scalecommander (`36`)** (*Mass Disintegrate*, *Bombardments*,
    *Maneuverability*) and **Flameshaper (`37`)** (*Engulf* consuming *Fire
    Breath* via `TickAura`, *Flame Siphon*).
- **Augmentation (`1473`):** *Ebon Might* (`ModKind::StatShare(Primary)` on
  party allies), *Prescience* (`CritChanceAdd` on allies), *Fire Breath*
  (empower), *Upheaval* (empower), *Eruption* (extends *Ebon Might*),
  *Breath of Eons*, *Blistering Scales*, *Time Skip* (`CooldownRatePct`).
  - Hero trees: **Scalecommander (`36`)** and **Chronowarden (`38`)**
    (*Chronoflame*, *Temporal Burst*, *Double-time*).
