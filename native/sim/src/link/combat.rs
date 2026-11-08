//! Link's weapons: the damage types of `DamageTable` (the order every enemy overlay uses), the
//! melee swing table `D_80854190` of `z_player.c` (hit window per swing, in animation frames) and
//! the spin-attack charge (`func_80844E3C`).
//!
//! Units: OoT runs at 20 Hz, Jumpman at 30 Hz, so an OoT frame is 1.5 ticks; model units are x4.4
//! (the Link rig is drawn at 0.044 = 4.4 x the actor's 0.01 scale).
use super::items::Sword;

pub const OOT_SCALE: f64 = 4.4;
/// 30 Hz ticks per 20 Hz OoT frame.
pub const TICKS_PER_FRAME: f64 = 1.5;

/// OoT units per frame -> Jumpman units per tick.
pub const fn speed(oot_units_per_frame: f64) -> f64 {
    oot_units_per_frame * OOT_SCALE / TICKS_PER_FRAME
}

/// OoT units per frame^2 -> Jumpman units per tick^2.
pub const fn accel(oot: f64) -> f64 {
    oot * OOT_SCALE / (TICKS_PER_FRAME * TICKS_PER_FRAME)
}

/// OoT length -> Jumpman length.
pub const fn len(oot: f64) -> f64 {
    oot * OOT_SCALE
}

/// OoT frames -> ticks.
pub const fn ticks(frames: f64) -> f64 {
    frames * TICKS_PER_FRAME
}

/// The 32 `DamageTable` rows in file order (`z_en_dekubaba.c` comments).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum Dmg {
    Nut,
    Stick,
    Slingshot,
    Explosive,
    Boomerang,
    Arrow,
    HammerSwing,
    Hookshot,
    KokiriSword,
    MasterSword,
    GiantSword,
    FireArrow,
    IceArrow,
    LightArrow,
    UnkArrow1,
    UnkArrow2,
    UnkArrow3,
    FireMagic,
    IceMagic,
    LightMagic,
    Shield,
    MirrorRay,
    KokiriSpin,
    GiantSpin,
    MasterSpin,
    KokiriJump,
    GiantJump,
    MasterJump,
    Unknown1,
    Unblockable,
    HammerJump,
    Unknown2,
}

impl Dmg {
    /// The Deku Baba's `sDamageTableNormal`, the table most small enemies copy: hit points removed
    /// by each damage type (1 heart = 2 points of a `DMG_ENTRY` in the enemy overlays).
    pub fn typical(self) -> i32 {
        use Dmg::*;
        match self {
            Nut => 0,
            Stick | Explosive | Boomerang | Arrow | HammerSwing | Hookshot => 2,
            Slingshot => 1,
            KokiriSword | KokiriSpin => 1,
            MasterSword | MasterSpin => 2,
            GiantSword | GiantSpin => 4,
            FireArrow | FireMagic => 4,
            IceArrow | LightArrow | UnkArrow1 | UnkArrow2 | UnkArrow3 => 2,
            KokiriJump => 2,
            MasterJump | HammerJump => 4,
            GiantJump => 8,
            IceMagic | LightMagic | Shield | MirrorRay | Unknown1 | Unblockable | Unknown2 => 0,
        }
    }

    pub fn is_fire(self) -> bool {
        matches!(self, Dmg::FireArrow | Dmg::FireMagic)
    }
    pub fn is_ice(self) -> bool {
        matches!(self, Dmg::IceArrow | Dmg::IceMagic)
    }
    pub fn is_explosive(self) -> bool {
        self == Dmg::Explosive
    }
    pub fn is_sword(self) -> bool {
        use Dmg::*;
        matches!(self, KokiriSword | MasterSword | GiantSword | KokiriSpin | GiantSpin | MasterSpin | KokiriJump | GiantJump | MasterJump)
    }
}

/// Which swing the sword makes (`PlayerMeleeWeaponAnimation`, the 1H set plus the extras).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Swing {
    /// `FORWARD_SLASH_1H` (Z-targeted or standing still).
    Forward,
    /// `RIGHT_SLASH_1H` (the horizontal slash).
    Right,
    Left,
    /// `STAB_1H`: Z-target + stick forward.
    Stab,
    /// Third hit of a combo (`*_COMBO_1H`).
    Combo(Direction),
    /// `FLIPSLASH_START` (A-jump while targeting, then B).
    FlipSlash,
    /// `JUMPSLASH_START` (B in the air).
    JumpSlash,
    Spin,
    BigSpin,
    /// Hammer overhead / side swing.
    HammerForward,
    HammerSide,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Direction {
    Forward,
    Right,
    Left,
    Stab,
}

/// One entry of `D_80854190`: animation clip played by `export-oot-link.py`, hit window in OoT
/// animation frames (inclusive) and the frame where the recovery clip takes over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwingInfo {
    pub clip: &'static str,
    pub hit_start: f64,
    pub hit_end: f64,
    /// Length of the swing clip in OoT frames (`Animation_GetLastFrame`), approximated by the exported clip at runtime.
    pub frames: f64,
}

impl Swing {
    /// `D_80854190` (the hit window columns). One-handed rows; the two-handed Biggoron versions
    /// share timings within a frame, so they reuse these.
    pub fn info(self) -> SwingInfo {
        let (clip, a, b, frames) = match self {
            Swing::Forward => ("slash", 1.0, 4.0, 8.0),
            Swing::Right => ("slashR", 1.0, 4.0, 8.0),
            Swing::Left => ("slashL", 0.0, 4.0, 8.0),
            Swing::Stab => ("stab", 0.0, 3.0, 8.0),
            Swing::Combo(Direction::Forward) => ("finish", 0.0, 5.0, 12.0),
            Swing::Combo(Direction::Right) => ("slashRFinish", 2.0, 8.0, 14.0),
            Swing::Combo(Direction::Left) => ("slashLFinish", 0.0, 6.0, 12.0),
            Swing::Combo(Direction::Stab) => ("stabFinish", 1.0, 9.0, 14.0),
            Swing::FlipSlash => ("flipSlash", 1.0, 10.0, 16.0),
            Swing::JumpSlash => ("jumpSlash", 1.0, 11.0, 16.0),
            Swing::Spin => ("spin", 0.0, 12.0, 20.0),
            Swing::BigSpin => ("bigSpin", 0.0, 16.0, 24.0),
            Swing::HammerForward => ("hammer", 3.0, 10.0, 18.0),
            Swing::HammerSide => ("hammerSide", 2.0, 11.0, 18.0),
        };
        SwingInfo { clip, hit_start: a, hit_end: b, frames }
    }

    /// Hit window in 30 Hz ticks since the swing began (inclusive start, exclusive end).
    pub fn window(self) -> (f64, f64) {
        let i = self.info();
        (ticks(i.hit_start), ticks(i.hit_end + 1.0))
    }

    pub fn total_ticks(self) -> f64 {
        ticks(self.info().frames)
    }

    pub fn is_jump(self) -> bool {
        matches!(self, Swing::FlipSlash | Swing::JumpSlash)
    }
    pub fn is_spin(self) -> bool {
        matches!(self, Swing::Spin | Swing::BigSpin)
    }
}

/// `D_80854488`: damage type for a swing with a given weapon (`[normal, jump]`), plus the spin
/// table `sSpinAttackDmgFlags` in `z_en_m_thunder.c`.
pub fn swing_damage(weapon: Weapon, swing: Swing) -> Dmg {
    use Dmg::*;
    let sword = |kokiri, master, giant| match weapon {
        Weapon::Sword(Sword::Kokiri) => kokiri,
        Weapon::Sword(Sword::Master) => master,
        Weapon::Sword(Sword::Biggoron) => giant,
        _ => master,
    };
    match (weapon, swing) {
        (Weapon::Hammer, Swing::FlipSlash | Swing::JumpSlash) => HammerJump,
        (Weapon::Hammer, _) => HammerSwing,
        (Weapon::Stick, Swing::FlipSlash | Swing::JumpSlash) => MasterJump,
        (Weapon::Stick, Swing::Spin | Swing::BigSpin) => Stick,
        (Weapon::Stick, _) => Stick,
        (_, Swing::FlipSlash | Swing::JumpSlash) => sword(KokiriJump, MasterJump, GiantJump),
        (_, Swing::Spin | Swing::BigSpin) => sword(KokiriSpin, MasterSpin, GiantSpin),
        _ => sword(KokiriSword, MasterSword, GiantSword),
    }
}

/// What Link is swinging (`Player_GetMeleeWeaponHeld`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Weapon {
    Sword(Sword),
    Stick,
    Hammer,
}

impl Weapon {
    /// Horizontal reach added to Link's body radius, Jumpman units. The blade lengths of the three
    /// swords are in the ratio 1 : 1.25 : 1.6 (approximate: measured from the exported rig).
    pub fn reach(self) -> f64 {
        match self {
            Weapon::Sword(Sword::Kokiri) => 70.0,
            Weapon::Sword(Sword::Master) => 90.0,
            Weapon::Sword(Sword::Biggoron) => 118.0,
            Weapon::Stick => 85.0,
            Weapon::Hammer => 105.0,
        }
    }

    pub fn two_handed(self) -> bool {
        matches!(self, Weapon::Sword(Sword::Biggoron) | Weapon::Hammer)
    }

    pub fn label(self) -> &'static str {
        match self {
            Weapon::Sword(s) => s.label(),
            Weapon::Stick => "DEKU STICK",
            Weapon::Hammer => "MEGATON HAMMER",
        }
    }
}

/// Spin-attack charge: `unk_858` steps toward 1.0 by 0.02 per OoT frame while B is held; a swing
/// finished by `>= 0.85` (or a stick circle) becomes the big spin.
pub const CHARGE_STEP: f64 = 0.02 / TICKS_PER_FRAME;
pub const CHARGE_BIG: f64 = 0.85;
/// The charge begins when the held-B check passes after this much charge (`unk_858 >= 0.1`).
pub const CHARGE_START: f64 = 0.1;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Charge {
    pub value: f64,
}

impl Charge {
    pub fn step(&mut self) {
        self.value = (self.value + CHARGE_STEP).min(1.0);
    }
    pub fn is_big(&self) -> bool {
        self.value >= CHARGE_BIG
    }
    /// Ticks of holding B until the charge is big (about 43 OoT frames = 64 ticks).
    pub fn ticks_to_big() -> f64 {
        (CHARGE_BIG / CHARGE_STEP).ceil()
    }
}

/// `Player_CanSpinAttack`: four consecutive control-stick octants make a circle.
/// `dirs` holds the last four octant indices (0..8); a full turn is four steps in the same sense
/// that cover at least 3 octants net... the original compares neighbours `(a+1) % 8` (or the
/// reverse) across the four samples.
pub fn stick_circle(dirs: &[i8; 4]) -> bool {
    if dirs.iter().any(|d| *d < 0) {
        return false;
    }
    let clockwise = dirs.windows(2).all(|w| (w[0] + 2) % 8 == w[1] || (w[0] + 1) % 8 == w[1]);
    let anti = dirs.windows(2).all(|w| (w[1] + 2) % 8 == w[0] || (w[1] + 1) % 8 == w[0]);
    let span = dirs.iter().map(|d| (d - dirs[0]).rem_euclid(8).min((dirs[0] - d).rem_euclid(8))).max().unwrap_or(0);
    (clockwise || anti) && span >= 3
}

/// Octant (0..8) of a stick deflection, or -1 inside the dead zone.
pub fn stick_octant(x: f64, y: f64) -> i8 {
    if x.hypot(y) < 20.0 {
        return -1;
    }
    let a = y.atan2(x).rem_euclid(std::f64::consts::TAU);
    (((a + std::f64::consts::TAU / 16.0) / (std::f64::consts::TAU / 8.0)) as i8) % 8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_tables_match_the_deku_baba_rows() {
        assert_eq!(Dmg::KokiriSword.typical(), 1);
        assert_eq!(Dmg::MasterSword.typical(), 2);
        assert_eq!(Dmg::GiantSword.typical(), 4);
        assert_eq!(Dmg::FireArrow.typical(), 4);
        assert_eq!(Dmg::Slingshot.typical(), 1);
        assert_eq!(Dmg::GiantJump.typical(), 8);
        assert_eq!(Dmg::KokiriJump.typical(), 2);
        assert_eq!(Dmg::Nut.typical(), 0);
        assert_eq!(Dmg::Unknown2 as u8, 31, "32 rows");
    }

    #[test]
    fn swing_damage_picks_spin_and_jump_rows() {
        let m = Weapon::Sword(Sword::Master);
        assert_eq!(swing_damage(m, Swing::Forward), Dmg::MasterSword);
        assert_eq!(swing_damage(m, Swing::Spin), Dmg::MasterSpin);
        assert_eq!(swing_damage(m, Swing::JumpSlash), Dmg::MasterJump);
        assert_eq!(swing_damage(Weapon::Sword(Sword::Biggoron), Swing::BigSpin), Dmg::GiantSpin);
        assert_eq!(swing_damage(Weapon::Sword(Sword::Kokiri), Swing::FlipSlash), Dmg::KokiriJump);
        assert_eq!(swing_damage(Weapon::Hammer, Swing::HammerForward), Dmg::HammerSwing);
        assert_eq!(swing_damage(Weapon::Hammer, Swing::JumpSlash), Dmg::HammerJump);
        assert_eq!(swing_damage(Weapon::Stick, Swing::Right), Dmg::Stick);
    }

    #[test]
    fn hit_windows_are_in_ticks() {
        let (a, b) = Swing::Forward.window();
        assert_eq!(a, 1.5);
        assert_eq!(b, 7.5);
        assert!(Swing::Spin.total_ticks() > Swing::Forward.total_ticks());
        assert!(Swing::Stab.window().0 == 0.0);
    }

    #[test]
    fn unit_conversion() {
        assert!((speed(150.0) - 440.0).abs() < 1e-9, "arrow speed 150/frame is 440 per tick");
        assert!((len(1.0) - 4.4).abs() < 1e-12);
        assert_eq!(ticks(20.0), 30.0);
    }

    #[test]
    fn charge_needs_about_two_seconds_to_go_big() {
        let mut c = Charge::default();
        let mut n = 0;
        while !c.is_big() {
            c.step();
            n += 1;
        }
        assert_eq!(n as f64, Charge::ticks_to_big());
        assert!((60.0..70.0).contains(&Charge::ticks_to_big()));
    }

    #[test]
    fn stick_circle_detects_a_turn() {
        assert!(stick_circle(&[0, 1, 2, 3]));
        assert!(stick_circle(&[6, 5, 4, 3]));
        assert!(!stick_circle(&[0, 0, 0, 0]));
        assert!(!stick_circle(&[0, 4, 0, 4]));
        assert!(!stick_circle(&[0, 1, -1, 3]));
        assert_eq!(stick_octant(60.0, 0.0), 0);
        assert_eq!(stick_octant(0.0, 60.0), 2);
        assert_eq!(stick_octant(1.0, 1.0), -1);
    }
}
