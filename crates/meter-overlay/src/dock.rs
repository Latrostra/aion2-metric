use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use serde::{Deserialize, Serialize};

use crate::persist;

pub const FILE_NAME: &str = "layout.json";
pub const SNAP_DISTANCE: i32 = 16;
const MIN_EDGE_OVERLAP: i32 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    fn right(&self) -> i32 {
        self.x + self.w as i32
    }

    fn bottom(&self) -> i32 {
        self.y + self.h as i32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Bottom,
    Top,
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dock {
    pub target: String,
    pub side: Side,
    pub offset: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelState {
    pub rect: Rect,
    pub dock: Option<Dock>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layout {
    pub panels: BTreeMap<String, PanelState>,
}

fn overlap_length(start_a: i32, end_a: i32, start_b: i32, end_b: i32) -> i32 {
    end_a.min(end_b) - start_a.max(start_b)
}

fn align_offset(offset: i32, moving_length: u32, target_length: u32) -> i32 {
    let far_edge_offset = target_length as i32 - moving_length as i32;
    if offset.abs() <= SNAP_DISTANCE {
        0
    } else if (offset - far_edge_offset).abs() <= SNAP_DISTANCE {
        far_edge_offset
    } else {
        offset
    }
}

pub fn nearest_dock(moving: Rect, targets: &[(String, Rect)]) -> Option<Dock> {
    let mut best: Option<(i32, Dock)> = None;
    for (label, target) in targets {
        let overlaps_horizontally =
            overlap_length(moving.x, moving.right(), target.x, target.right()) >= MIN_EDGE_OVERLAP;
        let overlaps_vertically =
            overlap_length(moving.y, moving.bottom(), target.y, target.bottom())
                >= MIN_EDGE_OVERLAP;
        let horizontal_offset = align_offset(moving.x - target.x, moving.w, target.w);
        let vertical_offset = align_offset(moving.y - target.y, moving.h, target.h);
        let candidates = [
            (
                overlaps_horizontally,
                (moving.y - target.bottom()).abs(),
                Side::Bottom,
                horizontal_offset,
            ),
            (
                overlaps_horizontally,
                (moving.bottom() - target.y).abs(),
                Side::Top,
                horizontal_offset,
            ),
            (
                overlaps_vertically,
                (moving.x - target.right()).abs(),
                Side::Right,
                vertical_offset,
            ),
            (
                overlaps_vertically,
                (moving.right() - target.x).abs(),
                Side::Left,
                vertical_offset,
            ),
        ];
        for (edges_overlap, distance, side, offset) in candidates {
            let closer_than_best = best
                .as_ref()
                .is_none_or(|(best_distance, _)| distance < *best_distance);
            if edges_overlap && distance <= SNAP_DISTANCE && closer_than_best {
                best = Some((
                    distance,
                    Dock {
                        target: label.clone(),
                        side,
                        offset,
                    },
                ));
            }
        }
    }
    best.map(|(_, dock)| dock)
}

pub fn docked_position(target: Rect, dock: &Dock, width: u32, height: u32) -> (i32, i32) {
    match dock.side {
        Side::Bottom => (target.x + dock.offset, target.bottom()),
        Side::Top => (target.x + dock.offset, target.y - height as i32),
        Side::Right => (target.right(), target.y + dock.offset),
        Side::Left => (target.x - width as i32, target.y + dock.offset),
    }
}

impl Layout {
    pub fn dependents(&self, label: &str) -> Vec<String> {
        let mut dependents: Vec<String> = Vec::new();
        let mut queue = vec![label.to_string()];
        let mut seen = HashSet::from([label.to_string()]);
        while let Some(parent) = queue.pop() {
            for (child, state) in &self.panels {
                if state
                    .dock
                    .as_ref()
                    .is_some_and(|dock| dock.target == parent)
                    && seen.insert(child.clone())
                {
                    dependents.push(child.clone());
                    queue.push(child.clone());
                }
            }
        }
        dependents
    }

    pub fn snap_targets(&self, label: &str, rects: &HashMap<String, Rect>) -> Vec<(String, Rect)> {
        let excluded: HashSet<String> = self
            .dependents(label)
            .into_iter()
            .chain([label.to_string()])
            .collect();
        let mut targets: Vec<(String, Rect)> = rects
            .iter()
            .filter(|(other, _)| !excluded.contains(*other))
            .map(|(other, rect)| (other.clone(), *rect))
            .collect();
        targets.sort_by(|(a, _), (b, _)| a.cmp(b));
        targets
    }

    pub fn follow_moves(
        &self,
        label: &str,
        rects: &HashMap<String, Rect>,
    ) -> Vec<(String, i32, i32)> {
        let mut current = rects.clone();
        let mut moves = Vec::new();
        for child in self.dependents(label) {
            let Some(state) = self.panels.get(&child) else {
                continue;
            };
            let (Some(dock), Some(child_rect)) = (&state.dock, current.get(&child).copied()) else {
                continue;
            };
            let Some(target) = current.get(&dock.target).copied() else {
                continue;
            };
            let (x, y) = docked_position(target, dock, child_rect.w, child_rect.h);
            if (x, y) != (child_rect.x, child_rect.y) {
                moves.push((child.clone(), x, y));
            }
            current.insert(child, Rect { x, y, ..child_rect });
        }
        moves
    }

    pub fn remove_panel(&mut self, label: &str) {
        self.panels.remove(label);
        for state in self.panels.values_mut() {
            if state.dock.as_ref().is_some_and(|dock| dock.target == label) {
                state.dock = None;
            }
        }
    }
}

pub fn load(dir: &Path) -> Layout {
    fs::read_to_string(dir.join(FILE_NAME))
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, layout: &Layout) -> io::Result<()> {
    let json = serde_json::to_vec_pretty(layout).map_err(io::Error::other)?;
    persist::write_atomically(dir, FILE_NAME, &json)
}

pub fn saver(dir: PathBuf) -> Sender<Layout> {
    persist::spawn_latest_value_saver(move |layout: &Layout| {
        if let Err(err) = save(&dir, layout) {
            eprintln!("could not save layout: {err}");
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: Rect = Rect {
        x: 1000,
        y: 100,
        w: 324,
        h: 222,
    };

    fn rects(list: &[(&str, Rect)]) -> HashMap<String, Rect> {
        list.iter()
            .map(|(label, rect)| (label.to_string(), *rect))
            .collect()
    }

    fn docked(target: &str, side: Side, offset: i32, rect: Rect) -> PanelState {
        PanelState {
            rect,
            dock: Some(Dock {
                target: target.into(),
                side,
                offset,
            }),
        }
    }

    #[test]
    fn dropping_near_the_bottom_edge_docks_aligned() {
        let pets = Rect {
            x: 1008,
            y: 330,
            w: 324,
            h: 180,
        };
        let dock = nearest_dock(pets, &[("main".into(), MAIN)]).unwrap();
        assert_eq!((dock.side, dock.offset), (Side::Bottom, 0));
        assert_eq!(docked_position(MAIN, &dock, pets.w, pets.h), (1000, 322));
    }

    #[test]
    fn right_side_and_far_edge_alignment() {
        let pets = Rect {
            x: 1330,
            y: 210,
            w: 200,
            h: 120,
        };
        let dock = nearest_dock(pets, &[("main".into(), MAIN)]).unwrap();
        assert_eq!(
            (dock.side, dock.offset),
            (Side::Right, 102),
            "bottoms aligned"
        );
        assert_eq!(docked_position(MAIN, &dock, pets.w, pets.h), (1324, 202));
    }

    #[test]
    fn far_away_or_without_overlap_does_not_dock() {
        let far = Rect {
            x: 1000,
            y: 400,
            w: 324,
            h: 180,
        };
        assert!(nearest_dock(far, &[("main".into(), MAIN)]).is_none());
        let diagonal = Rect {
            x: 1330,
            y: 330,
            w: 200,
            h: 120,
        };
        assert!(nearest_dock(diagonal, &[("main".into(), MAIN)]).is_none());
    }

    #[test]
    fn moving_the_parent_drags_the_whole_chain() {
        let pets = Rect {
            x: 1000,
            y: 322,
            w: 324,
            h: 180,
        };
        let dps = Rect {
            x: 1324,
            y: 322,
            w: 200,
            h: 180,
        };
        let mut layout = Layout::default();
        layout
            .panels
            .insert("panel-pets".into(), docked("main", Side::Bottom, 0, pets));
        layout.panels.insert(
            "panel-dps".into(),
            docked("panel-pets", Side::Right, 0, dps),
        );
        let moved_main = Rect {
            x: 500,
            y: 50,
            ..MAIN
        };
        let all = rects(&[
            ("main", moved_main),
            ("panel-pets", pets),
            ("panel-dps", dps),
        ]);
        let mut moves = layout.follow_moves("main", &all);
        moves.sort();
        assert_eq!(
            moves,
            vec![
                ("panel-dps".to_string(), 824, 272),
                ("panel-pets".to_string(), 500, 272)
            ]
        );
        assert_eq!(
            layout.snap_targets("panel-pets", &all),
            vec![("main".to_string(), moved_main)],
            "a window never docks to its own children"
        );
    }

    #[test]
    fn closing_a_window_undocks_its_children_and_layout_roundtrips() {
        let mut layout = Layout::default();
        layout
            .panels
            .insert("panel-pets".into(), docked("main", Side::Bottom, 0, MAIN));
        layout.panels.insert(
            "panel-dps".into(),
            docked("panel-pets", Side::Right, 0, MAIN),
        );
        layout.remove_panel("panel-pets");
        assert_eq!(layout.panels.len(), 1);
        assert!(layout.panels["panel-dps"].dock.is_none());

        let dir = std::env::temp_dir().join(format!("a2m-layout-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(load(&dir), Layout::default());
        save(&dir, &layout).unwrap();
        assert_eq!(load(&dir), layout);
        let _ = fs::remove_dir_all(&dir);
    }
}
