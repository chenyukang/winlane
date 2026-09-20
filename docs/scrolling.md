# Mouse Scrolling

Open **Settings → Mouse Scrolling** and turn on **Enable custom scrolling**. Changes save and apply immediately. New and existing installations leave this feature disabled until you enable it.

- **Mouse:** reverse vertical and horizontal scrolling independently. Vertical reversal is selected by default.
- **Wheel step:** `0` keeps the system step size. Values `1–100` scale single-notch vertical mouse wheel events; `3` is a useful starting point. Accelerated multi-line events retain their original scale.
- **Trackpad:** independent vertical/horizontal reversal, both off by default. Its smooth scrolling and inertia retain their original scale, as does Magic Mouse scrolling.

Directions are relative to macOS's **Natural Scrolling** setting; Winlane does not change that preference. Quit Scroll Reverser or other scroll modifiers before enabling this feature to avoid applying two transformations.

The page shows whether scrolling is active. If it cannot start, allow Winlane in **System Settings → Privacy & Security → Accessibility / Input Monitoring**, then click **Retry**. Turning the feature off or quitting Winlane removes its listeners and immediately restores unmodified system events. Settings are restored on the next launch.

## Implementation

Two session event taps separate passive gesture observation from scroll filtering. Multi-finger touch events identify trackpad scrolling; continuous scrolling alone cannot distinguish a trackpad from Magic Mouse. Momentum keeps its initiating device even if a discrete wheel event arrives in between. This classification is heuristic; third-party drivers or simultaneous touch-device use can be ambiguous. Application-generated events are left alone.

Filtering updates line, point and fixed-point deltas together, preserving fractional movement. Where available, optional macOS HID accessors also update the attached deltas consumed by WebKit. Without those accessors, standard CoreGraphics filtering remains available, but some WebKit gestures may behave differently. No preferences, file I/O or device discovery runs in the event callbacks. Disabled settings install no event taps.

The touch-based approach and HID compatibility were informed by [Scroll Reverser's implementation](https://github.com/pilotmoon/Scroll-Reverser/blob/master/MouseTap.m). Winlane implements its settings, event lifecycle and transformations in Rust.
