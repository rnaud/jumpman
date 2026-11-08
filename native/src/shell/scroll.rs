//! Scrolling for the menus: mouse wheel, touch drag (with a little inertia) and keeping the
//! keyboard / gamepad focus inside the visible part of a scroll area.
//!
//! bevy_ui 0.16 lays out `Overflow::scroll_y()` containers and clamps their [`ScrollPosition`],
//! but has no input handling, so this module supplies it. Scroll offsets survive screen rebuilds
//! through [`ScrollMemory`] (the menus are rebuilt from scratch whenever a selection changes).
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::input::touch::Touches;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use std::collections::HashMap;

/// A scrollable menu region; `id` keys [`ScrollMemory`].
#[derive(Component, Clone, Copy, Debug)]
pub struct ScrollArea {
    pub id: u8,
}

/// Last scroll offset (layout px) of each [`ScrollArea::id`].
#[derive(Resource, Default, Debug)]
pub struct ScrollMemory(pub HashMap<u8, f32>);

/// The finger / mouse button that is currently down, for tap-vs-drag decisions in `nav::pointer`.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct PointerDrag {
    pub active: bool,
    pub touch: bool,
    /// Moved far enough that this is a scroll, not a tap (touch only).
    pub dragged: bool,
    pub start: Vec2,
    pub last: Vec2,
}

/// Set by keyboard / gamepad navigation: scroll the focused button into view once.
#[derive(Resource, Default)]
pub struct RevealFocus(pub bool);

/// Logical px a finger may wander and still count as a tap.
pub const TAP_SLOP: f32 = 10.0;

/// A finger that moved this far from where it came down is scrolling, not tapping.
pub fn is_drag(start: Vec2, now: Vec2) -> bool {
    now.distance(start) > TAP_SLOP
}

/// Fresh scroll state for an area being (re)built.
pub fn restored(memory: &ScrollMemory, id: u8) -> ScrollPosition {
    ScrollPosition { offset_x: 0.0, offset_y: memory.0.get(&id).copied().unwrap_or(0.0) }
}

pub fn remember(mut memory: ResMut<ScrollMemory>, areas: Query<(&ScrollArea, &ScrollPosition)>) {
    for (area, pos) in &areas {
        if memory.0.get(&area.id) != Some(&pos.offset_y) {
            memory.0.insert(area.id, pos.offset_y);
        }
    }
}

/// Track the primary pointer; runs before `nav::pointer` reads the result.
pub fn track_pointer(touches: Res<Touches>, mouse: Res<ButtonInput<MouseButton>>, mut drag: ResMut<PointerDrag>) {
    if let Some(t) = touches.iter_just_pressed().next() {
        *drag = PointerDrag { active: true, touch: true, dragged: false, start: t.start_position(), last: t.position() };
    } else if mouse.just_pressed(MouseButton::Left) && !drag.active {
        *drag = PointerDrag { active: true, touch: false, dragged: false, ..*drag };
    }
    if drag.active && drag.touch {
        if let Some(t) = touches.iter().next() {
            drag.last = t.position();
            if is_drag(drag.start, drag.last) {
                drag.dragged = true;
            }
        } else {
            drag.active = false;
        }
    } else if drag.active && !mouse.pressed(MouseButton::Left) {
        drag.active = false;
    }
}

type Areas<'w, 's> = Query<'w, 's, (Entity, &'static mut ScrollPosition, &'static ComputedNode, &'static GlobalTransform), With<ScrollArea>>;

fn contains(node: &ComputedNode, at: &GlobalTransform, p: Vec2) -> bool {
    let half = node.size() / 2.0;
    let c = at.translation().truncate();
    p.x >= c.x - half.x && p.x <= c.x + half.x && p.y >= c.y - half.y && p.y <= c.y + half.y
}

/// The smallest scroll area under a physical-px point.
fn area_at(areas: &Areas, p: Vec2) -> Option<Entity> {
    areas.iter().filter(|(_, _, n, g)| contains(n, g, p)).min_by(|a, b| (a.2.size().x * a.2.size().y).total_cmp(&(b.2.size().x * b.2.size().y))).map(|a| a.0)
}

fn max_offset(node: &ComputedNode) -> f32 {
    ((node.content_size().y - node.size().y) * node.inverse_scale_factor()).max(0.0)
}

pub fn wheel_scroll(mut wheel: EventReader<MouseWheel>, windows: Query<&Window, With<PrimaryWindow>>, mut areas: Areas, ui_scale: Res<UiScale>) {
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else {
        wheel.clear();
        return;
    };
    let physical = cursor * window.scale_factor();
    for ev in wheel.read() {
        let step = match ev.unit {
            MouseScrollUnit::Line => 42.0,
            MouseScrollUnit::Pixel => 1.0,
        };
        if let Some(target) = area_at(&areas, physical) {
            if let Ok((_, mut pos, node, _)) = areas.get_mut(target) {
                pos.offset_y = (pos.offset_y - ev.y * step / ui_scale.0.max(1e-3)).clamp(0.0, max_offset(node));
            }
        }
    }
}

#[derive(Default)]
pub struct Fling {
    target: Option<Entity>,
    velocity: f32,
}

/// Drag a finger over a scroll area to scroll it; let go with speed and it coasts.
pub fn touch_scroll(
    drag: Res<PointerDrag>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut areas: Areas,
    ui_scale: Res<UiScale>,
    time: Res<Time<Real>>,
    mut fling: Local<Fling>,
    mut prev: Local<Option<(bool, Vec2)>>,
) {
    let Ok(window) = windows.single() else { return };
    let dt = time.delta_secs().clamp(1e-3, 0.1);
    let scale = ui_scale.0.max(1e-3);
    let was = prev.take();
    *prev = Some((drag.active && drag.touch, drag.last));
    if drag.active && drag.touch {
        if !was.is_some_and(|w| w.0) {
            // Finger just came down: pick the area it landed on.
            fling.target = area_at(&areas, drag.start * window.scale_factor());
            fling.velocity = 0.0;
            return;
        }
        let Some(target) = fling.target else { return };
        if !drag.dragged {
            return;
        }
        let dy = (drag.last.y - was.map_or(drag.last.y, |w| w.1.y)) / scale;
        if let Ok((_, mut pos, node, _)) = areas.get_mut(target) {
            pos.offset_y = (pos.offset_y - dy).clamp(0.0, max_offset(node));
        }
        fling.velocity = fling.velocity * 0.6 + (-dy / dt) * 0.4;
    } else if let Some(target) = fling.target {
        if fling.velocity.abs() < 20.0 {
            fling.target = None;
            return;
        }
        if let Ok((_, mut pos, node, _)) = areas.get_mut(target) {
            let next = (pos.offset_y + fling.velocity * dt).clamp(0.0, max_offset(node));
            if next == pos.offset_y {
                fling.velocity = 0.0;
            }
            pos.offset_y = next;
        }
        fling.velocity *= 0.5f32.powf(dt / 0.18);
    }
}

/// After keyboard / gamepad focus moves, scroll just enough to show the focused button.
pub fn scroll_to_focus(
    focus: Res<super::nav::Focus>,
    mut reveal: ResMut<RevealFocus>,
    nodes: Query<(&ComputedNode, &GlobalTransform)>,
    parents: Query<&ChildOf>,
    mut areas: Areas,
) {
    if !reveal.0 {
        return;
    }
    let Some(entity) = focus.0 else {
        reveal.0 = false;
        return;
    };
    let Ok((node, at)) = nodes.get(entity) else { return };
    if node.size() == Vec2::ZERO {
        return; // not laid out yet; try again next frame
    }
    reveal.0 = false;
    let mut cursor = entity;
    while let Ok(parent) = parents.get(cursor) {
        cursor = parent.parent();
        if let Ok((_, mut pos, area_node, area_at)) = areas.get_mut(cursor) {
            let (top, bottom) = (at.translation().y - node.size().y / 2.0, at.translation().y + node.size().y / 2.0);
            let (area_top, area_bottom) = (area_at.translation().y - area_node.size().y / 2.0, area_at.translation().y + area_node.size().y / 2.0);
            let margin = 6.0;
            let delta = if top - margin < area_top {
                top - margin - area_top
            } else if bottom + margin > area_bottom {
                bottom + margin - area_bottom
            } else {
                0.0
            };
            if delta != 0.0 {
                pos.offset_y = (pos.offset_y + delta * area_node.inverse_scale_factor()).clamp(0.0, max_offset(area_node));
            }
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_restores_offsets() {
        let mut m = ScrollMemory::default();
        assert_eq!(restored(&m, 3).offset_y, 0.0);
        m.0.insert(3, 120.0);
        assert_eq!(restored(&m, 3).offset_y, 120.0);
    }

    #[test]
    fn a_short_touch_is_a_tap_a_long_one_is_a_scroll() {
        assert!(!is_drag(Vec2::new(100.0, 100.0), Vec2::new(104.0, 106.0)));
        assert!(is_drag(Vec2::new(100.0, 100.0), Vec2::new(100.0, 140.0)));
    }

    #[test]
    fn the_mouse_button_is_tracked_but_never_counts_as_a_drag() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).init_resource::<Touches>().init_resource::<ButtonInput<MouseButton>>().init_resource::<PointerDrag>().add_systems(PreUpdate, track_pointer);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Left);
        app.update();
        let d = *app.world().resource::<PointerDrag>();
        assert!(d.active && !d.touch && !d.dragged);
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().release(MouseButton::Left);
        app.update();
        assert!(!app.world().resource::<PointerDrag>().active);
    }
}
