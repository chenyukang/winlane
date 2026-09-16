use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use std::collections::{HashMap, HashSet};
use winlane::discovery::normal_window_surface;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWindowListCopyWindowInfo(options: u32, relative_to: u32) -> CFArrayRef;
}

#[derive(Default)]
pub struct Inventory {
    pub normal: HashMap<i32, HashSet<u32>>,
    known: HashSet<u32>,
}

impl Inventory {
    pub fn read() -> Self {
        // SAFETY: All windows, excluding desktop surfaces. This reads metadata,
        // not pixels or window titles, and does not request screen recording.
        let raw = unsafe { CGWindowListCopyWindowInfo(1 << 4, 0) };
        if raw.is_null() {
            return Self::default();
        }
        // SAFETY: Copy returns a retained CF array, owned here until drop.
        let array = unsafe { CFArray::<*const std::ffi::c_void>::wrap_under_create_rule(raw) };
        let mut inventory = Self::default();
        for raw in array.iter() {
            // SAFETY: Every array entry is a live CF object; retain and type-check it.
            let value = unsafe { CFType::wrap_under_get_rule(*raw) };
            let Some(dict) = value.downcast_into::<CFDictionary>() else {
                continue;
            };
            let Some(id) = integer(&dict, "kCGWindowNumber").and_then(|id| u32::try_from(id).ok())
            else {
                continue;
            };
            inventory.known.insert(id);
            let Some(pid) =
                integer(&dict, "kCGWindowOwnerPID").and_then(|pid| i32::try_from(pid).ok())
            else {
                continue;
            };
            let Some(bounds) = value_for(&dict, "kCGWindowBounds")
                .and_then(|value| value.downcast_into::<CFDictionary>())
            else {
                continue;
            };
            if let (Some(layer), Some(alpha), Some(width), Some(height)) = (
                integer(&dict, "kCGWindowLayer"),
                number(&dict, "kCGWindowAlpha"),
                number(&bounds, "Width"),
                number(&bounds, "Height"),
            ) && normal_window_surface(layer, alpha, width, height)
            {
                inventory.normal.entry(pid).or_default().insert(id);
            }
        }
        inventory
    }

    pub fn rejects(&self, pid: i32, id: u32) -> bool {
        self.known.contains(&id) && !self.normal.get(&pid).is_some_and(|ids| ids.contains(&id))
    }
}

fn value_for(dict: &CFDictionary, name: &str) -> Option<CFType> {
    let name = CFString::new(name);
    let raw = *dict.find(name.as_CFTypeRef())?;
    // SAFETY: The dictionary owns this live CF value; the Get rule retains it.
    Some(unsafe { CFType::wrap_under_get_rule(raw) })
}

fn number(dict: &CFDictionary, name: &str) -> Option<f64> {
    value_for(dict, name)?.downcast_into::<CFNumber>()?.to_f64()
}

fn integer(dict: &CFDictionary, name: &str) -> Option<i64> {
    value_for(dict, name)?.downcast_into::<CFNumber>()?.to_i64()
}

#[cfg(test)]
mod tests {
    use super::Inventory;

    #[test]
    #[ignore = "requires a graphical macOS session with open application windows"]
    fn live_window_inventory_contains_usable_window_ids() {
        let inventory = Inventory::read();
        assert!(!inventory.known.is_empty());
        assert!(!inventory.normal.is_empty());
        let mut counts: Vec<_> = inventory
            .normal
            .iter()
            .map(|(pid, ids)| {
                assert!(*pid > 0);
                assert!(
                    ids.iter()
                        .all(|id| *id != 0 && inventory.known.contains(id))
                );
                (*pid, ids.len())
            })
            .collect();
        counts.sort_unstable();
        eprintln!("WindowServer normal-window counts (PID, count): {counts:?}");
    }
}
