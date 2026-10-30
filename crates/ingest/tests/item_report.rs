//! Reading weapons from SimC's `debug=1` item lines.

use portunus_core::SimDuration;
use portunus_gamedata::item::{GearSlot, WeaponDef};
use portunus_ingest::SimcProfile;

const PROFILE: &str = "rogue=\"r\"\nspec=subtlety\nmain_hand=zathatek,id=271093\noff_hand=farstriders_mercy,id=237837\n";

const REPORT: &str = "\
0.000 name=zathatek id=271093 slot=main_hand quality=epic ilevel=344 type=one_hand/dagger stats={ +103 Agi } damage={ 148 - 275 } speed=1.800 enchant={ x }
0.000 name=farstriders_mercy id=237837 slot=off_hand quality=epic ilevel=331 type=one_hand/dagger stats={ +92 Agi } damage={ 140 - 234 } speed=1.800
0.000 name=ring id=1 slot=finger1 quality=rare ilevel=334 type=armor/misc stats={ +2199 Sta }
";

#[test]
fn weapons_come_from_simcs_item_lines() {
    let mut profile = SimcProfile::parse_str(PROFILE);
    profile.read_item_report(REPORT);
    let weapon = |slot| profile.slots[&slot].weapon;
    assert_eq!(
        weapon(GearSlot::MainHand),
        Some(WeaponDef {
            speed: SimDuration(1800),
            min_damage: 148.0,
            max_damage: 275.0,
            ranged: false,
        })
    );
    assert_eq!(
        weapon(GearSlot::OffHand).map(|w| (w.min_damage, w.max_damage)),
        Some((140.0, 234.0))
    );
}
