//! Gem Clash, SSB64 items (engine-free): with `Config::smash_items` a share of the item drops are Super Smash Bros.
//! items. Each rides one of Gem Clash's own item mechanics (so every character picks them up and the bots already know
//! how to use them) with SSB64's numbers and SSB64 knockback (`smash_knockback`, through [`Brawl::smash_hit`]):
//!
//! | SSB64 item | plays as | hit |
//! |---|---|---|
//! | Beam Sword, Home-Run Bat, Fan, Star Rod, Hammer | the hammer's swing (B) | the item's swing |
//! | Ray Gun, Fire Flower | the blaster's shots (B) | `LGunAmmo` / `FFlowerFlame` weapon numbers |
//! | Green Shell, Red Shell | a throwable (B / Z) | the shell (`ITAttributes` damage) |
//! | Bob-omb, Motion-Sensor Bomb | a bomb (lit on pick-up) | an SSB64 blast |
//! | Maxim Tomato, Heart Container | a heart (touch) | heals half / all |
//! | Star | a bubble (touch) | invincible for `ITSTAR_INVINCIBLE_TIME` |
//!
//! The spawn table ([`SMASH_ITEM_TABLE`]) and the share of drops ([`SMASH_ITEM_SHARE`]) are data for the balance
//! harness. The models come from the import (`characters::smash_models`, drawn from `BrawlBridge::smash_items`).
use super::*;
use jumpman_sim::smash::items::ItemKind as SItem;
use jumpman_sim::smash::{HitKind, SmashHit};
use jumpman_sim::smash_hitbox::{Element, Knockback};

/// Percent of the item drops that are SSB64 items when they are on.
pub const SMASH_ITEM_SHARE: u32 = 55;
/// Which SSB64 items drop and how often (weights).
pub const SMASH_ITEM_TABLE: [(SItem, u32); 14] = [
    (SItem::BeamSword, 4),
    (SItem::HomeRunBat, 2),
    (SItem::Fan, 3),
    (SItem::StarRod, 3),
    (SItem::RayGun, 3),
    (SItem::FireFlower, 3),
    (SItem::Hammer, 1),
    (SItem::MaximTomato, 2),
    (SItem::Heart, 1),
    (SItem::Star, 1),
    (SItem::BobOmb, 3),
    (SItem::GreenShell, 3),
    (SItem::RedShell, 2),
    (SItem::MotionBomb, 2),
];
/// `ITSTAR_INVINCIBLE_TIME` (600 frames).
pub const STAR_TICKS: u16 = 300;

/// The Gem Clash item an SSB64 item plays as.
pub fn base_kind(s: SItem) -> ItemKind {
    match s {
        SItem::BeamSword | SItem::HomeRunBat | SItem::Fan | SItem::StarRod | SItem::Hammer => ItemKind::Hammer,
        SItem::RayGun | SItem::FireFlower => ItemKind::Blaster,
        SItem::MaximTomato | SItem::Heart => ItemKind::Heal,
        SItem::Star => ItemKind::Shield,
        SItem::BobOmb | SItem::MotionBomb => ItemKind::Bomb,
        SItem::Crate | SItem::Capsule => ItemKind::Crate,
        SItem::Barrel => ItemKind::Barrel,
        _ => ItemKind::Rock,
    }
}

/// Uses of a held SSB64 item (swings, shots).
pub fn uses(s: SItem) -> u8 {
    match s {
        SItem::BeamSword | SItem::StarRod => 8,
        SItem::HomeRunBat => 3,
        SItem::Fan => 14,
        SItem::Hammer => 6,
        SItem::RayGun => 16,
        SItem::FireFlower => 30,
        _ => 1,
    }
}

fn kb(angle: i32, growth: u32, fixed: u32, base: u32, damage: u32, element: Element) -> Knockback {
    Knockback { angle, growth, fixed, base, element, damage, shield_damage: 0, hit_ground: true, hit_air: true, can_rebound: false, sfx_kind: 0, sfx_level: 1 }
}

/// The SSB64 numbers of an item's hit: a swing, a shot (Ray Gun `dITCommonData_LGunAmmo`, Fire Flower `FFlowerFlame`),
/// a thrown item (its `ITAttributes` damage) or a blast. Swings: the item's smash swing (Gem Clash's one swing).
pub fn item_knockback(s: SItem) -> Knockback {
    match s {
        SItem::BeamSword => kb(361, 100, 0, 20, 14, Element::Slash),
        SItem::HomeRunBat => kb(40, 100, 0, 70, 24, Element::Normal),
        SItem::Fan => kb(361, 20, 0, 4, 2, Element::Normal),
        SItem::StarRod => kb(361, 100, 0, 10, 10, Element::Normal),
        SItem::Hammer => kb(361, 100, 0, 30, 20, Element::Normal),
        SItem::RayGun => kb(70, 40, 0, 50, 7, Element::Electric),
        SItem::FireFlower => kb(0, 100, 3, 0, 3, Element::Fire),
        SItem::BobOmb | SItem::MotionBomb => kb(80, 100, 0, 40, 22, Element::Fire),
        s => kb(361, 100, 0, 15, s.damage() as u32, Element::Normal),
    }
}

/// A Gem Clash hit from an SSB64 item.
pub fn item_hit(s: SItem, dir: V2, pos: [f32; 3], fighter: SmashFighter) -> SmashHit {
    let k = item_knockback(s);
    SmashHit {
        damage: k.damage as f32,
        kind: jumpman_sim::smash::hit_kind(k.element),
        dir: [dir[0] as f64 * 0.8, 0.6, dir[1] as f64 * 0.8],
        knock: 10.0 + k.damage as f64,
        pos: [pos[0] as f64, pos[1] as f64, pos[2] as f64],
        fighter,
        mv: Some(jumpman_sim::smash::SpecialMove::Item),
        knockback: Some(k),
        motion: None,
    }
}

impl Brawl {
    /// Turn the item just spawned in `slot` into an SSB64 item (when they are on, [`SMASH_ITEM_SHARE`] of the time).
    pub(super) fn maybe_smash_item(&mut self, slot: Option<usize>) {
        let Some(k) = slot else { return };
        if !self.cfg.smash_items || self.rng.below(100) as u32 >= SMASH_ITEM_SHARE {
            return;
        }
        let total: u32 = SMASH_ITEM_TABLE.iter().map(|t| t.1).sum();
        let mut r = self.rng.below(total as usize) as u32;
        let mut pick = SMASH_ITEM_TABLE[0].0;
        for (s, w) in SMASH_ITEM_TABLE {
            if r < w {
                pick = s;
                break;
            }
            r -= w;
        }
        self.make_smash_item(k, pick);
    }

    /// Test / app hook: item `k` becomes SSB64 item `s`.
    pub fn make_smash_item(&mut self, k: usize, s: SItem) {
        let it = &mut self.items[k];
        it.kind = base_kind(s);
        it.ssb = Some(s);
        it.uses = uses(s);
        it.fuse = 0;
    }

    fn attacker_fighter(&self, a: i8) -> SmashFighter {
        if a >= 0 { self.f[a as usize].smash.unwrap_or(SmashFighter::Mario) } else { SmashFighter::Mario }
    }

    /// A held SSB64 weapon's swing: everyone in its reach in front takes the item's hit.
    pub(super) fn smash_item_swing(&mut self, i: usize, s: SItem) {
        let me = self.f[i];
        let reach = match s {
            SItem::Fan => 150.0,
            SItem::HomeRunBat | SItem::Hammer => 200.0,
            _ => 230.0,
        } + (me.size - 1.0) * 60.0;
        let fwd = dir_of(me.yaw);
        for j in 0..self.n {
            let o = self.f[j];
            if j == i || !o.alive || o.team == me.team || self.f[i].hit_mask & (1 << j) != 0 {
                continue;
            }
            let d = [o.pos[0] - me.pos[0], o.pos[2] - me.pos[2]];
            let dist = len2(d);
            let dn = norm2(d, fwd);
            if dist > reach + o.radius() || (dn[0] * fwd[0] + dn[1] * fwd[1] < 0.0 && dist > o.radius() * 1.1) || (o.pos[1] - me.pos[1]).abs() > 140.0 * me.size.max(o.size) {
                continue;
            }
            self.f[i].hit_mask |= 1 << j;
            let hit = item_hit(s, dn, o.pos, self.attacker_fighter(i as i8));
            self.smash_hit(j, i, &hit);
        }
        let k = item_knockback(s);
        self.melee_mons(i, reach, 0.0, k.damage as f32, 700.0, 300.0);
    }

    /// A projectile / thrown SSB64 item reached fighter `j`.
    pub(super) fn smash_item_strike(&mut self, j: usize, owner: i8, s: SItem, dir: V2, pos: [f32; 3]) -> bool {
        let hit = item_hit(s, dir, pos, self.attacker_fighter(owner));
        if owner < 0 {
            // nobody's (a dropped item rolling): plain damage
            return self.hit(j, -1, hit.damage, dir, 300.0, 120.0);
        }
        self.smash_hit(j, owner as usize, &hit) != jumpman_sim::smash::HitOutcome::Miss
    }

    /// An SSB64 bomb's blast: SSB64 knockback for everyone in it (falling off with distance), the usual for monsters.
    pub(super) fn smash_item_blast(&mut self, pos: [f32; 3], r: f32, owner: i8, s: SItem) {
        self.ev.push(Ev::Boom { pos, r });
        let base = item_knockback(s);
        for j in 0..self.n {
            let f = self.f[j];
            if !f.alive || (owner >= 0 && self.f[owner as usize].team == f.team && owner as usize != j) {
                continue;
            }
            let d = dist2([pos[0], pos[2]], pos_of(&f));
            if d > r + f.radius() || (f.pos[1] - pos[1]).abs() > 240.0 {
                continue;
            }
            let dir = norm2([f.pos[0] - pos[0], f.pos[2] - pos[2]], [0.0, 1.0]);
            let mut hit = item_hit(s, dir, pos, self.attacker_fighter(owner));
            hit.damage = base.damage as f32 * (1.0 - 0.5 * (d / r).min(1.0));
            hit.kind = HitKind::Explosion;
            let was = self.f[j].inv;
            self.f[j].inv = 0;
            let a = if owner >= 0 { owner as usize } else { j };
            if self.smash_hit(j, a, &hit) == jumpman_sim::smash::HitOutcome::Miss {
                self.f[j].inv = was;
            }
        }
        self.explode_mons(pos, r, base.damage as f32, 900.0, owner);
        for k in 0..MAXI {
            let it = &mut self.items[k];
            if it.alive && it.state == IState::Rest && dist2([it.pos[0], it.pos[2]], [pos[0], pos[2]]) < r {
                let dir = norm2([it.pos[0] - pos[0], it.pos[2] - pos[2]], [1.0, 0.0]);
                it.vel = [dir[0] * 700.0, 600.0, dir[1] * 700.0];
            }
        }
    }
}

#[cfg(test)]
#[path = "brawl_items_tests.rs"]
mod tests;
