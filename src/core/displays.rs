#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn contains(self, (x, y): (f64, f64)) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Display {
    pub id: u32,
    pub frame: Rect,
    pub visible: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub display_id: u32,
    pub x: f64,
    pub y: f64,
    pub receives_keyboard: bool,
}

pub fn active_display(
    displays: &[Display],
    pointer: (f64, f64),
    focused_window_center: Option<(f64, f64)>,
    main_display: Option<u32>,
) -> Option<u32> {
    displays
        .iter()
        .find(|display| display.frame.contains(pointer))
        .or_else(|| {
            focused_window_center.and_then(|center| {
                displays
                    .iter()
                    .find(|display| display.frame.contains(center))
            })
        })
        .or_else(|| {
            displays
                .iter()
                .find(|display| Some(display.id) == main_display)
        })
        .or_else(|| displays.first())
        .map(|display| display.id)
}

pub fn placements(
    displays: &[Display],
    panel_size: (f64, f64),
    pointer: (f64, f64),
    focused_display: Option<u32>,
) -> Vec<Placement> {
    let mut distinct: Vec<&Display> = Vec::new();
    for display in displays {
        if !distinct
            .iter()
            .any(|other| other.id == display.id || other.frame == display.frame)
        {
            distinct.push(display);
        }
    }
    let focused = distinct
        .iter()
        .find(|display| Some(display.id) == focused_display)
        .or_else(|| {
            distinct
                .iter()
                .find(|display| display.frame.contains(pointer))
        })
        .or_else(|| distinct.first())
        .map(|display| display.id);
    distinct
        .into_iter()
        .map(|display| Placement {
            display_id: display.id,
            x: display.visible.x + ((display.visible.width - panel_size.0) / 2.0).max(0.0),
            y: display.visible.y + ((display.visible.height - panel_size.1) * 0.58).max(0.0),
            receives_keyboard: Some(display.id) == focused,
        })
        .collect()
}
