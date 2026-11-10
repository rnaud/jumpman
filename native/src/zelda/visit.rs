//! Port of `KokiriVisit` (`kokiriVillage.ts`): talking to residents/Saria/Mido/signs, the shop
//! counter, treasure chests, the warp pad, the Great Deku Tree meetings, the ladder and the
//! crawlspace. Engine-free: the plugin feeds it the player, the pressed buttons and the save, and
//! reads `textbox` / `events` / `prompt` / `frame_conversation` back.
use super::save::{treasure_reward, ZeldaSave, WARP_DESTINATIONS};
use super::scene::{KokiriScene, TalkSpot, TreasureChest, ZeldaScene};
use super::textbox::{MessagePage, OotTextbox};
use crate::link::icons::{chest_icon, Icon};
use jumpman_sim::link::items::{Shield, Sword};
use jumpman_sim::math::Vec3;
use jumpman_sim::mario_runtime::MarioRuntime;
use jumpman_sim::state::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ShopItem {
    Shield,
    Heart,
}

#[derive(Clone, Debug)]
struct Climb {
    from: Vec3,
    to: Vec3,
    tick: u32,
    crawling: bool,
}

/// Things the view/audio layers have to react to.
#[derive(Clone, Debug, PartialEq)]
pub enum VisitEvent {
    /// A chest was opened (play its lid animation).
    OpenLid(String),
    /// The warp pad menu picked a destination course.
    Travel(String),
}

fn page(text: impl Into<String>, style: u32) -> MessagePage {
    MessagePage { text: text.into(), style, branches: None }
}

fn dist2(a: Vec3, b: Vec3) -> f64 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

/// How long a "you got" card stays up without input before it dismisses itself (ms).
pub const CARD_MS: f64 = 5500.0;
/// A tap or key that opens a card must not also dismiss it: the earliest dismissal after the card appears (ms).
pub const CARD_GRACE_MS: f64 = 350.0;

/// The "you got" card: the item's picture and name over the message box.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemCard {
    pub icon: Icon,
    pub name: String,
    pub since_ms: f64,
}

pub struct KokiriVisit {
    pub textbox: OotTextbox,
    /// Set while an item was just received; cleared with the message (`clear`).
    pub card: Option<ItemCard>,
    /// Monotonic milliseconds, set by the caller before each step (typewriter clock).
    pub clock_ms: f64,
    pub events: Vec<VisitEvent>,
    pages: Vec<MessagePage>,
    index: usize,
    conversation_flag: Option<String>,
    visitor: String,
    speaking: Option<TalkSpot>,
    offer: Option<ShopItem>,
    approving_mido: bool,
    tree_meeting: Option<Vec3>,
    accepting_tree: bool,
    warp_menu: Option<(Vec<(&'static str, &'static str)>, usize)>,
    climb: Option<Climb>,
}

impl Default for KokiriVisit {
    fn default() -> Self {
        Self {
            textbox: OotTextbox::default(),
            card: None,
            clock_ms: 0.0,
            events: vec![],
            pages: vec![],
            index: 0,
            conversation_flag: None,
            visitor: "Link".into(),
            speaking: None,
            offer: None,
            approving_mido: false,
            tree_meeting: None,
            accepting_tree: false,
            warp_menu: None,
            climb: None,
        }
    }
}

impl KokiriVisit {
    fn treasure_at<'a>(&self, scene: &'a KokiriScene, pos: Vec3, save: &ZeldaSave) -> Option<&'a TreasureChest> {
        let mut nearest = None;
        let mut distance = 260.0;
        for chest in &scene.treasure_chests {
            let d = dist2(chest.pos, pos);
            if d < distance && (chest.pos[1] - pos[1]).abs() < 180.0 && !save.chest_opened(&chest.key) {
                nearest = Some(chest);
                distance = d;
            }
        }
        nearest
    }

    fn on_warp_pad(&self, scene: &KokiriScene, pos: Vec3) -> bool {
        scene.warp_pad.as_ref().is_some_and(|w| (pos[1] - w.pos[1]).abs() < 160.0 && dist2(pos, w.pos) < 235.0)
    }

    fn show_warp_menu(&mut self) {
        let Some((dests, pg)) = &self.warp_menu else { return };
        let mut labels: Vec<&str> = dests.iter().skip(pg * 2).take(2).map(|d| d.1).collect();
        labels.push(if *pg == 0 { "More destinations" } else { "Previous destinations" });
        let text = format!("Travel to (B to cancel)\n\x1c{}", labels.join("\n"));
        let mut p = page(text, 3);
        p.branches = Some(vec![vec![], vec![], vec![]]);
        let now = self.clock_ms;
        self.textbox.show("Warp pad", p, false, true, now);
    }

    fn near_tree(&self, scene: &KokiriScene, pos: Vec3, save: &ZeldaSave) -> bool {
        let Some(q) = &scene.quest else { return false };
        let m = q.mouth.meeting;
        save.deku_path_open && !save.has_spoken_to("dekuTreeAccepted") && (pos[1] - m[1]).abs() < 650.0 && dist2(pos, m) < 2600.0
    }

    fn show_current(&mut self) {
        let Some(pg) = self.pages.get(self.index).cloned() else { return };
        let target = self.speaking.as_ref();
        let speaker = if self.tree_meeting.is_some() {
            "Great Deku Tree".to_string()
        } else {
            match target {
                Some(t) => t.label.clone().unwrap_or_else(|| match t.kind.as_str() {
                    "saria" => "Saria".into(),
                    "sign" => "Sign".into(),
                    _ => "Kokiri Shop".into(),
                }),
                None => "Kokiri Sword".into(),
            }
        };
        let name = if target.is_some_and(|t| t.kind == "sign") { "Link" } else { &self.visitor };
        let shown = MessagePage { text: pg.text.replace("{name}", name), ..pg };
        let has_next = self.index + 1 < self.pages.len();
        let now = self.clock_ms;
        self.textbox.show(&speaker, shown, has_next, false, now);
    }

    fn shelf(&self, scene: &KokiriScene, pos: Vec3) -> Option<ShopItem> {
        let counter = scene.counter.as_ref()?;
        let npc = self.nearest(scene, pos);
        let mut best = npc.map_or(245.0, |n| dist2(n.pos, pos));
        let mut selected = None;
        for (item, p) in [(ShopItem::Shield, counter.shield), (ShopItem::Heart, counter.heart)] {
            let d = dist2(p, pos);
            if (p[1] - pos[1]).abs() < 140.0 && d < best {
                best = d;
                selected = Some(item);
            }
        }
        selected
    }

    fn crawl_end(&self, scene: &KokiriScene, pos: Vec3) -> Option<usize> {
        scene.quest.as_ref()?.crawl.iter().position(|p| (p[1] - pos[1]).abs() < 85.0 && dist2(*p, pos) < 160.0)
    }

    fn near_chest(&self, scene: &KokiriScene, pos: Vec3, save: &ZeldaSave) -> bool {
        scene.quest.as_ref().is_some_and(|q| !save.sword && (q.chest[1] - pos[1]).abs() < 180.0 && dist2(q.chest, pos) < 260.0)
    }

    fn ladder_end(&self, scene: &KokiriScene, pos: Vec3) -> Option<usize> {
        scene.ladder.as_ref()?.iter().position(|p| (p[1] - pos[1]).abs() < 80.0 && dist2(*p, pos) < 160.0)
    }

    fn give_card(&mut self, icon: Icon, name: &str) {
        self.card = Some(ItemCard { icon, name: name.to_string(), since_ms: self.clock_ms });
    }

    /// True when a tap anywhere may advance the open message: a plain message (no choices, no shop offer, no warp menu) that has
    /// been up long enough. Touch players get the same dismissal as A.
    pub fn tap_advances(&self) -> bool {
        let Some(page) = &self.textbox.page else { return false };
        let settled = self.card.as_ref().is_some_and(|c| self.clock_ms - c.since_ms >= CARD_GRACE_MS);
        settled && self.offer.is_none() && self.warp_menu.is_none() && page.branches.is_none()
    }

    pub fn clear(&mut self) {
        self.card = None;
        self.climb = None;
        self.warp_menu = None;
        self.pages.clear();
        self.offer = None;
        self.approving_mido = false;
        self.accepting_tree = false;
        self.tree_meeting = None;
        self.textbox.hide();
        self.speaking = None;
        self.conversation_flag = None;
    }

    fn nearest<'a>(&self, scene: &'a KokiriScene, pos: Vec3) -> Option<&'a TalkSpot> {
        let mut nearest = None;
        let mut distance = f64::INFINITY;
        for spot in &scene.talks {
            let d = dist2(spot.pos, pos);
            if d < distance && d < spot.range.unwrap_or(240.0) && (spot.pos[1] - pos[1]).abs() < spot.height.unwrap_or(155.0) {
                nearest = Some(spot);
                distance = d;
            }
        }
        nearest
    }

    /// Conversation, shop, warp menu or climb in progress: the player must not be simulated.
    pub fn busy(&self) -> bool {
        !self.pages.is_empty() || self.offer.is_some() || self.warp_menu.is_some() || self.climb.is_some()
    }

    pub fn speaker_id(&self) -> Option<&str> {
        self.speaking.as_ref().map(|s| s.id.as_deref().unwrap_or(&s.kind))
    }

    /// NPC animation to play while speaking.
    pub fn gesture(&self) -> &'static str {
        match self.speaker_id() {
            Some("mido") if self.index >= 2 => {
                if self.index >= 5 {
                    "Annoyed"
                } else {
                    "Curious"
                }
            }
            Some("saria") if (3..=5).contains(&self.index) => "Surprise",
            _ => "Talk",
        }
    }

    /// `frameConversation`: (eye, look-at) while a conversation owns the camera.
    pub fn frame_conversation(&self, player: Vec3) -> Option<(Vec3, Vec3)> {
        if let Some([x, y, z]) = self.tree_meeting {
            return Some(([x - 2600.0, y + 480.0, z + 1495.0], [x + 500.0, y + 1180.0, z - 280.0]));
        }
        let sp = self.speaking.as_ref()?;
        if sp.kind == "sign" {
            return None;
        }
        if let Some(p) = sp.portrait {
            let eye = [p.center[0] + p.distance * p.yaw.sin(), p.center[1] + p.distance * 0.12, p.center[2] + p.distance * p.yaw.cos()];
            return Some((eye, p.center));
        }
        let [x, y, z] = sp.pos;
        let angle = (player[0] - x).atan2(player[2] - z) + 1.08;
        Some(([x + angle.sin() * 380.0, y + 175.0, z + angle.cos() * 380.0], [x, y + 134.0, z]))
    }

    /// The on-screen action hint.
    pub fn prompt(&self, scene: &KokiriScene, pos: Vec3, save: &ZeldaSave) -> Option<String> {
        if self.offer.is_some() || !self.pages.is_empty() || self.climb.is_some() || self.warp_menu.is_some() {
            return None;
        }
        if self.treasure_at(scene, pos, save).is_some() {
            return Some("SPACE / A - OPEN CHEST".into());
        }
        if self.on_warp_pad(scene, pos) {
            return Some("SPACE / A - WARP".into());
        }
        if self.near_tree(scene, pos, save) {
            return Some("SPACE / A - SPEAK TO THE GREAT DEKU TREE".into());
        }
        if self.near_chest(scene, pos, save) {
            return Some("SPACE / A - OPEN CHEST".into());
        }
        if self.crawl_end(scene, pos).is_some() {
            return Some("SPACE / A - ENTER CRAWLSPACE".into());
        }
        if let Some(end) = self.ladder_end(scene, pos) {
            return Some(if end == 1 { "SPACE / A - CLIMB DOWN" } else { "SPACE / A - CLIMB UP" }.into());
        }
        if let Some(shelf) = self.shelf(scene, pos) {
            return Some(if shelf == ShopItem::Shield { "SPACE / A - DEKU SHIELD - 40 RUPEES" } else { "SPACE / A - RECOVERY HEART - 10 RUPEES" }.into());
        }
        self.nearest(scene, pos).map(|t| if t.kind == "sign" { "SPACE / A - READ SIGN" } else { "SPACE / A - TALK" }.into())
    }

    fn halt(player: &mut MarioRuntime) {
        player.set_action(ACT_IDLE, 0.0);
        player.forward_vel = 0.0;
        player.vel = [0.0; 3];
    }

    /// One 30 Hz step. Returns true when the visit consumed the tick (the player is not simulated).
    pub fn step(&mut self, scene: &ZeldaScene, player: &mut MarioRuntime, buttons: u32, name: &str, save: &mut ZeldaSave, stick_y: f64) -> bool {
        let kokiri = &scene.kokiri;
        let pressed = buttons & A_BUTTON != 0;
        let pos = player.pos;
        let now = self.clock_ms;
        // A "you got" message dismisses itself after a few seconds (the last page only: a longer conversation waits for the player).
        if self.card.as_ref().is_some_and(|c| now - c.since_ms > CARD_MS) && self.pages.len() <= self.index + 1 && self.offer.is_none() {
            self.clear();
            return true;
        }
        if self.warp_menu.is_some() {
            self.textbox.choose(stick_y);
            if buttons & B_BUTTON != 0 {
                self.clear();
            } else if pressed {
                let (dests, pg) = self.warp_menu.clone().unwrap();
                if self.textbox.choice == 2 {
                    self.warp_menu = Some((dests, 1 - pg));
                    self.show_warp_menu();
                } else if let Some(dest) = dests.get(pg * 2 + self.textbox.choice) {
                    self.clear();
                    self.events.push(VisitEvent::Travel(dest.0.to_string()));
                }
            }
            return true;
        }
        if pressed && self.pages.is_empty() && self.offer.is_none() && self.climb.is_none() {
            let chest = self.treasure_at(kokiri, pos, save).cloned();
            if chest.is_some() || self.on_warp_pad(kokiri, pos) {
                Self::halt(player);
                if let Some(chest) = chest {
                    if save.open_treasure_chest(&chest.key, chest.item) {
                        super::music::queue_jingle(crate::audio::oot_music::JINGLE_OPEN_CHEST);
                        let reward = treasure_reward(chest.item).unwrap();
                        if let Some(grant) = reward.grant {
                            grant(&mut player.link.inv);
                        }
                        match reward.key {
                            1 => player.link.inv.small_keys += 1,
                            2 => player.link.inv.boss_key = true,
                            _ => {}
                        }
                        if reward.heal {
                            player.health = player.max_health.min(player.health + 0x100 as f64);
                        }
                        self.events.push(VisitEvent::OpenLid(chest.key.clone()));
                        let text = if chest.item == 5 && player.link.enabled {
                            // Link carries the slingshot on a C button, there is no E key to press.
                            "Fairy Slingshot! 30 Deku Seeds.\nPut it on a C button in the item screen.".to_string()
                        } else if chest.item == 5 {
                            "Fairy Slingshot! 30 Deku Seeds.\nEquip it with the E key.".to_string()
                        } else if reward.rupees > 0 {
                            format!("You found a {}!\nWallet: {}/99 rupees.", reward.name, save.rupees)
                        } else if reward.heal {
                            "You found a Recovery Heart!".to_string()
                        } else {
                            format!("You found the {}!", reward.name)
                        };
                        self.pages = vec![page(text, 3)];
                        self.index = 0;
                        let p = self.pages[0].clone();
                        self.textbox.show("Treasure chest", p, false, false, now);
                        self.give_card(chest_icon(chest.item), reward.name);
                    }
                } else if let Some(pad) = &kokiri.warp_pad {
                    let dests: Vec<_> = WARP_DESTINATIONS.iter().copied().filter(|d| d.0 != pad.course).collect();
                    self.warp_menu = Some((dests, 0));
                    self.show_warp_menu();
                }
                return true;
            }
        }
        // The village NPCs have solid cylinders: keep the rigs apart.
        for npc in &kokiri.talks {
            if (npc.kind != "saria" && npc.id.as_deref() != Some("mido")) || (player.pos[1] - npc.pos[1]).abs() > 150.0 {
                continue;
            }
            let (dx, dz) = (player.pos[0] - npc.pos[0], player.pos[2] - npc.pos[2]);
            if dx.hypot(dz) < 100.0 {
                let angle = dx.atan2(dz);
                player.pos[0] = npc.pos[0] + 100.0 * angle.sin();
                player.pos[2] = npc.pos[2] + 100.0 * angle.cos();
            }
        }
        let pos = player.pos;
        let end = self.ladder_end(kokiri, pos);
        if pressed && self.pages.is_empty() && self.climb.is_none() {
            if let Some(end) = end {
                self.climb = Some(Climb { from: pos, to: kokiri.ladder.unwrap()[1 - end], tick: 0, crawling: false });
                player.set_action(ACT_CLIMBING_POLE, 0.0);
                player.vel = [0.0; 3];
                player.forward_vel = 0.0;
                player.face_angle[1] = 0.0;
            }
        }
        let crawl_end = self.crawl_end(kokiri, pos);
        if pressed && self.pages.is_empty() && self.climb.is_none() {
            if let Some(end) = crawl_end {
                self.climb = Some(Climb { from: pos, to: kokiri.quest.as_ref().unwrap().crawl[1 - end], tick: 0, crawling: true });
                player.set_action(ACT_CRAWLING, 0.0);
                player.forward_vel = 0.0;
                player.vel = [0.0; 3];
                player.face_angle[1] = if end == 0 { 0.0 } else { -32768.0 };
            }
        }
        if let Some(climb) = &mut self.climb {
            climb.tick += 1;
            let t = climb.tick as f64 / if climb.crawling { 100.0 } else { 48.0 };
            if climb.crawling {
                player.intended_mag = 16.0;
            }
            player.anim_frame += 1.0;
            for i in 0..3 {
                player.pos[i] = climb.from[i] + (climb.to[i] - climb.from[i]) * t;
            }
            if t >= 1.0 {
                self.climb = None;
                player.set_action(ACT_IDLE, 0.0);
            }
            return true;
        }
        if let Some(item) = self.offer {
            if buttons & B_BUTTON != 0 {
                self.clear();
            } else if pressed {
                self.offer = None;
                let cost = if item == ShopItem::Shield { 40 } else { 10 };
                let message = if item == ShopItem::Shield && save.shield {
                    "You already own a Deku Shield.".to_string()
                } else if item == ShopItem::Heart && player.health >= player.max_health {
                    "Your health is full.".to_string()
                } else if !save.buy(if item == ShopItem::Shield { "shield" } else { "heart" }, cost) {
                    format!("You need {} rupees. You have {}.", cost, save.rupees)
                } else if item == ShopItem::Shield {
                    self.give_card(Icon::Shield(Shield::Deku), "Deku Shield");
                    "You got a Deku Shield!".to_string()
                } else {
                    player.health = player.max_health.min(player.health + 0x100 as f64);
                    self.give_card(Icon::Heart, "Recovery Heart");
                    "Recovered one health wedge.".to_string()
                };
                self.pages = vec![page(message, 3)];
                self.index = 0;
                let p = self.pages[0].clone();
                self.textbox.show("Kokiri Shop", p, false, true, now);
            }
            return true;
        }
        if !self.pages.is_empty() {
            self.textbox.choose(stick_y);
            if !pressed || self.textbox.finish_reveal() {
                return true;
            }
            let branch = self.pages[self.index].branches.as_ref().and_then(|b| b.get(self.textbox.choice)).cloned();
            if let Some(branch) = branch {
                let at = self.index + 1;
                for (i, p) in branch.into_iter().enumerate() {
                    self.pages.insert(at + i, p);
                }
                if self.tree_meeting.is_some() {
                    self.accepting_tree = self.textbox.choice == 0;
                }
            }
            self.index += 1;
            if self.index >= self.pages.len() {
                if self.accepting_tree {
                    save.finish_conversation("dekuTreeAccepted");
                }
                if self.approving_mido {
                    save.open_deku_path();
                }
                let flag = self.conversation_flag.clone();
                if flag.as_deref() == Some("dekuFarewell") {
                    save.clear_deku_challenge("emerald");
                }
                if let Some(flag) = flag {
                    save.finish_conversation(&flag);
                }
                self.clear();
            } else {
                self.show_current();
            }
            return true;
        }
        if let Some(q) = &kokiri.quest {
            if save.deku_cleared("gohma") && !save.deku_cleared("emerald") && dist2(pos, q.mouth.meeting) < 3200.0 {
                self.tree_meeting = Some(q.mouth.meeting);
                self.visitor = name.to_string();
                self.conversation_flag = Some("dekuFarewell".into());
                self.pages = scene.farewell.clone();
                self.index = 0;
                self.show_current();
                return true;
            }
        }
        let met_tree = save.has_spoken_to("dekuTreeMeeting");
        if let Some(q) = &kokiri.quest {
            if self.near_tree(kokiri, pos, save) && (pressed || !met_tree) {
                let mouth = &q.mouth;
                self.tree_meeting = Some(mouth.meeting);
                self.conversation_flag = Some("dekuTreeMeeting".into());
                self.pages = if met_tree { mouth.retry.clone() } else { mouth.first.clone() };
                self.index = 0;
                self.visitor = name.to_string();
                Self::halt(player);
                player.face_angle[1] = ((mouth.closed[0] - pos[0]).atan2(mouth.closed[2] - pos[2]) * 32768.0 / std::f64::consts::PI).round();
                self.show_current();
                return true;
            }
        }
        if !pressed {
            return false;
        }
        if let Some(q) = &kokiri.quest {
            if self.near_chest(kokiri, pos, save) && save.collect_kokiri_sword() {
                self.pages = q.reward.clone();
                self.index = 0;
                Self::halt(player);
                let first = self.pages[0].clone();
                let has_next = self.pages.len() > 1;
                self.speaking = None;
                self.textbox.show("Kokiri Sword", first, has_next, false, now);
                self.give_card(Icon::Sword(Sword::Kokiri), "Kokiri Sword");
                return true;
            }
        }
        if let Some(shelf) = self.shelf(kokiri, pos) {
            self.offer = Some(shelf);
            player.set_action(ACT_IDLE, 0.0);
            player.forward_vel = 0.0;
            player.vel[0] = 0.0;
            player.vel[2] = 0.0;
            let what = if shelf == ShopItem::Shield { "Deku Shield - 40" } else { "Recovery Heart - 10" };
            let text = format!("{what} rupees.\nYour wallet holds {}.\nA: Buy      B: Cancel", save.rupees);
            self.textbox.show("Kokiri Shop", page(text, 3), false, true, now);
            return true;
        }
        let Some(target) = self.nearest(kokiri, pos).cloned() else { return false };
        let saria = target.kind == "saria";
        self.visitor = name.to_string();
        let after_mido = if saria && save.has_spoken_to("mido") { kokiri.after_mido.as_ref() } else { None };
        self.conversation_flag = target.id.clone().or_else(|| saria.then(|| if after_mido.is_some() { "sariaAfterMido".to_string() } else { "saria".to_string() }));
        let met = self.conversation_flag.as_deref().is_some_and(|f| save.has_spoken_to(f));
        self.pages = if let Some(am) = after_mido {
            am[met as usize].clone()
        } else if met && target.repeat.is_some() {
            target.repeat.clone().unwrap()
        } else if saria {
            if met {
                kokiri.repeat.clone().unwrap_or_default()
            } else {
                let mut p = kokiri.greeting.clone().unwrap_or_default();
                p.extend(target.pages.clone());
                p
            }
        } else {
            target.pages.clone()
        };
        if target.id.as_deref() == Some("mido") {
            if let Some(q) = &kokiri.quest {
                self.approving_mido = !save.deku_path_open && save.sword && save.shield;
                if self.approving_mido {
                    self.pages = q.approved.clone();
                } else if save.deku_path_open {
                    self.pages = q.passed.clone();
                }
            }
        }
        self.index = 0;
        player.forward_vel = 0.0;
        player.vel[0] = 0.0;
        player.vel[2] = 0.0;
        player.face_angle[1] = ((target.pos[0] - pos[0]).atan2(target.pos[2] - pos[2]) * 32768.0 / std::f64::consts::PI).round();
        self.speaking = Some(target);
        player.set_action(ACT_IDLE, 0.0);
        if self.pages.is_empty() {
            self.clear();
            return false;
        }
        self.show_current();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::world::CourseRequest;
    use crate::game::runner::GameSim;
    use std::path::PathBuf;

    fn assets() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
    }

    fn setup(course: &str) -> (GameSim, ZeldaScene, ZeldaSave, KokiriVisit) {
        let sim = GameSim::load(&assets(), CourseRequest::new(course)).unwrap();
        (sim, ZeldaScene::load(&assets(), course, 1).unwrap(), ZeldaSave::default(), KokiriVisit::default())
    }

    #[test]
    fn talking_to_a_resident_walks_the_pages() {
        let (mut sim, scene, mut save, mut visit) = setup("kokiri_forest");
        let spot = scene.kokiri.talks.iter().find(|t| t.kind == "resident").unwrap().clone();
        sim.mario.pos = [spot.pos[0] + 100.0, spot.pos[1], spot.pos[2]];
        assert_eq!(visit.prompt(&scene.kokiri, sim.mario.pos, &save).as_deref(), Some("SPACE / A - TALK"));
        assert!(!visit.step(&scene, &mut sim.mario, 0, "Link", &mut save, 0.0));
        assert!(visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0));
        assert!(visit.busy() && visit.textbox.shown());
        let n = spot.pages.len();
        for _ in 0..n {
            visit.textbox.finish_reveal();
            assert!(visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0));
        }
        assert!(!visit.busy() && !visit.textbox.shown());
        assert!(save.has_spoken_to(spot.id.as_deref().unwrap()));
    }

    #[test]
    fn saria_greets_then_waits() {
        let (mut sim, scene, mut save, mut visit) = setup("kokiri_forest");
        let saria = scene.kokiri.talks.iter().find(|t| t.kind == "saria").unwrap().clone();
        sim.mario.pos = [saria.pos[0] + 120.0, saria.pos[1], saria.pos[2]];
        visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
        assert!(visit.pages.len() > 2);
        assert!(visit.pages[0].text.contains("Yahoo"));
        visit.clear();
        save.finish_conversation("saria");
        visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
        assert_eq!(visit.pages.len(), scene.kokiri.repeat.as_ref().unwrap().len());
    }

    #[test]
    fn sword_chest_gives_the_sword() {
        let (mut sim, scene, mut save, mut visit) = setup("kokiri_forest");
        let chest = scene.kokiri.quest.as_ref().unwrap().chest;
        sim.mario.pos = [chest[0], chest[1], chest[2] + 150.0];
        assert_eq!(visit.prompt(&scene.kokiri, sim.mario.pos, &save).as_deref(), Some("SPACE / A - OPEN CHEST"));
        assert!(visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0));
        assert!(save.sword);
    }

    #[test]
    fn shop_sells_shield_and_checks_wallet() {
        let (mut sim, scene, mut save, mut visit) = setup("kokiri_shop");
        let shelf = scene.kokiri.counter.as_ref().unwrap().shield;
        sim.mario.pos = [shelf[0], shelf[1], shelf[2] + 60.0];
        // Nobody else is closer than the shelf.
        assert!(visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0));
        assert!(visit.busy());
        visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
        assert!(visit.textbox.page.as_ref().unwrap().text.contains("You need 40 rupees"));
        visit.clear();
        save.rupees = 50;
        visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
        visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
        assert!(save.shield);
        assert_eq!(save.rupees, 10);
    }

    /// Put the player next to the chest with `key` and return the chest.
    fn stand_at_chest(sim: &mut GameSim, scene: &ZeldaScene, key: &str) {
        let c = scene.kokiri.treasure_chests.iter().find(|c| c.key == key).unwrap();
        sim.mario.pos = [c.pos[0], c.pos[1], c.pos[2] + 150.0];
    }

    #[test]
    fn a_chest_item_shows_its_card_and_a_dismisses_it() {
        let (mut sim, scene, mut save, mut visit) = setup("dodongos_cavern");
        stand_at_chest(&mut sim, &scene, "dodongos_cavern:8");
        assert!(visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0));
        let card = visit.card.clone().expect("the item card is up");
        assert_eq!(card.icon, Icon::DungeonMap);
        assert_eq!(card.name, "Dungeon Map");
        assert!(visit.textbox.shown() && visit.busy());
        // The first press finishes the typewriter, the next closes the message and the card with it.
        visit.clock_ms = 100.0;
        visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
        visit.clock_ms = 200.0;
        visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
        assert!(!visit.busy() && !visit.textbox.shown() && visit.card.is_none());
        // Standing there and pressing A again cannot re-open the chest: it is flagged as opened.
        assert!(!visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0) || visit.card.is_none());
        assert!(visit.card.is_none());
    }

    #[test]
    fn a_card_dismisses_itself_after_a_few_seconds_and_the_player_is_free_again() {
        let (mut sim, scene, mut save, mut visit) = setup("dodongos_cavern");
        stand_at_chest(&mut sim, &scene, "dodongos_cavern:8");
        visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
        assert!(visit.card.is_some());
        visit.clock_ms = CARD_MS - 1.0;
        assert!(visit.step(&scene, &mut sim.mario, 0, "Link", &mut save, 0.0), "still up, still consuming the tick");
        assert!(visit.card.is_some());
        visit.clock_ms = CARD_MS + 1.0;
        visit.step(&scene, &mut sim.mario, 0, "Link", &mut save, 0.0);
        assert!(visit.card.is_none() && !visit.busy() && !visit.textbox.shown());
        // The next tick is the player's own again.
        assert!(!visit.step(&scene, &mut sim.mario, 0, "Link", &mut save, 0.0));
    }

    #[test]
    fn a_tap_advances_a_card_only_after_the_grace_period() {
        let (mut sim, scene, mut save, mut visit) = setup("dodongos_cavern");
        assert!(!visit.tap_advances());
        stand_at_chest(&mut sim, &scene, "dodongos_cavern:8");
        visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
        visit.clock_ms = CARD_GRACE_MS - 1.0;
        assert!(!visit.tap_advances(), "the tap that opened the chest must not also close it");
        visit.clock_ms = CARD_GRACE_MS + 1.0;
        assert!(visit.tap_advances());
        // A tap is an A press: reveal first, then close.
        visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
        visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
        assert!(!visit.busy() && !visit.tap_advances());
    }

    #[test]
    fn two_chests_in_a_row_each_get_their_card() {
        let (mut sim, scene, mut save, mut visit) = setup("dodongos_cavern");
        for (i, key) in ["dodongos_cavern:8", "dodongos_cavern:5"].into_iter().enumerate() {
            stand_at_chest(&mut sim, &scene, key);
            visit.clock_ms = 1000.0 * i as f64;
            visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
            let name = visit.card.as_ref().map(|c| c.name.clone());
            assert_eq!(name.as_deref(), Some(if i == 0 { "Dungeon Map" } else { "Compass" }));
            visit.textbox.finish_reveal();
            visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
            assert!(visit.card.is_none() && !visit.busy());
        }
        assert!(save.chest_opened("dodongos_cavern:8") && save.chest_opened("dodongos_cavern:5"));
    }

    #[test]
    fn the_shop_shield_gets_a_card_too() {
        let (mut sim, scene, mut save, mut visit) = setup("kokiri_shop");
        let shelf = scene.kokiri.counter.as_ref().unwrap().shield;
        sim.mario.pos = [shelf[0], shelf[1], shelf[2] + 60.0];
        save.rupees = 50;
        visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
        assert!(visit.card.is_none() && !visit.tap_advances(), "the offer is not a card and a tap must not buy");
        visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
        assert_eq!(visit.card.as_ref().map(|c| c.icon), Some(Icon::Shield(Shield::Deku)));
        visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
        assert!(visit.card.is_none() && !visit.busy());
    }

    #[test]
    fn mido_approves_once_sword_and_shield_are_owned() {
        let (mut sim, scene, mut save, mut visit) = setup("kokiri_forest");
        let mido = scene.kokiri.talks.iter().find(|t| t.id.as_deref() == Some("mido")).unwrap().clone();
        sim.mario.pos = [mido.pos[0] + 130.0, mido.pos[1], mido.pos[2]];
        save.sword = true;
        save.shield = true;
        visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
        let n = visit.pages.len();
        assert_eq!(n, scene.kokiri.quest.as_ref().unwrap().approved.len());
        for _ in 0..n {
            visit.textbox.finish_reveal();
            visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
        }
        assert!(save.deku_path_open);
    }

    #[test]
    fn ladder_climb_moves_the_player() {
        let (mut sim, scene, mut save, mut visit) = setup("kokiri_forest");
        let ladder = scene.kokiri.ladder.unwrap();
        sim.mario.pos = ladder[0];
        assert!(visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0));
        for _ in 0..60 {
            visit.step(&scene, &mut sim.mario, 0, "Link", &mut save, 0.0);
        }
        assert!(dist2(sim.mario.pos, ladder[1]) < 1.0 && !visit.busy());
    }

    #[test]
    fn warp_pad_menu_travels() {
        let (mut sim, scene, mut save, mut visit) = setup("graveyard");
        let pad = scene.kokiri.warp_pad.as_ref().unwrap().pos;
        sim.mario.pos = pad;
        assert!(visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0));
        assert!(visit.busy());
        visit.step(&scene, &mut sim.mario, A_BUTTON, "Link", &mut save, 0.0);
        assert_eq!(visit.events, vec![VisitEvent::Travel("sacred_forest_meadow".into())]);
    }
}
