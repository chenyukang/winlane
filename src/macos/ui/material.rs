use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2::{AnyThread, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSArray, NSObjectProtocol, NSPoint, NSRect};

define_class!(
    // SAFETY: This view draws a static tint above AppKit's native material on the main thread.
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    struct PanelSurface;
    unsafe impl NSObjectProtocol for PanelSurface {}
    impl PanelSurface {
        #[unsafe(method(drawRect:))]
        fn draw(&self, dirty: NSRect) {
            unsafe { let _: () = msg_send![super(self), drawRect: dirty]; }
            let dark = unsafe {
                self.effectiveAppearance().bestMatchFromAppearancesWithNames(
                    &NSArray::from_slice(&[NSAppearanceNameAqua, NSAppearanceNameDarkAqua]),
                ).is_some_and(|name| &*name == NSAppearanceNameDarkAqua)
            };
            let (start, end) = if dark {
                (tint(0x172039, 0.88), tint(0x102b32, 0.80))
            } else {
                (tint(0xf5f7ff, 0.94), tint(0xecf8fa, 0.88))
            };
            if let Some(gradient) = NSGradient::initWithStartingColor_endingColor(NSGradient::alloc(), &start, &end) {
                gradient.drawInRect_angle(self.bounds(), -25.0);
            }
        }
    }
);

pub(crate) fn glass_available() -> bool {
    AnyClass::get(c"NSGlassEffectView").is_some()
}

enum PanelMaterial {
    Glass(Retained<NSGlassEffectView>),
    Frosted {
        container: Retained<NSView>,
        blur: Retained<NSVisualEffectView>,
    },
}

pub(crate) struct PanelBackdrop {
    pub(crate) content: Retained<NSView>,
    material: PanelMaterial,
}

impl PanelBackdrop {
    pub(crate) fn new(frame: NSRect, mtm: MainThreadMarker) -> Self {
        Self::with_glass(frame, mtm, glass_available())
    }

    fn with_glass(frame: NSRect, mtm: MainThreadMarker, glass: bool) -> Self {
        let resize = NSAutoresizingMaskOptions::ViewWidthSizable
            | NSAutoresizingMaskOptions::ViewHeightSizable;
        let bounds = NSRect::new(NSPoint::ZERO, frame.size);
        let content = NSView::initWithFrame(NSView::alloc(mtm), bounds);
        content.setAutoresizingMask(resize);
        let material = if glass {
            // Resolve availability before touching the class on older macOS.
            let view = NSGlassEffectView::initWithFrame(NSGlassEffectView::alloc(mtm), frame);
            view.setStyle(NSGlassEffectViewStyle::Regular);
            view.setCornerRadius(18.0);
            view.setContentView(Some(&content));
            PanelMaterial::Glass(view)
        } else {
            let container = NSView::initWithFrame(NSView::alloc(mtm), frame);
            let blur = frosted_backdrop(bounds, mtm);
            container.addSubview(&blur);
            container.addSubview(&content);
            PanelMaterial::Frosted { container, blur }
        };
        let backdrop = Self { content, material };
        backdrop.view().setAutoresizingMask(resize);
        backdrop
    }

    pub(crate) fn view(&self) -> &NSView {
        match &self.material {
            PanelMaterial::Glass(view) => view,
            PanelMaterial::Frosted { container, .. } => container,
        }
    }

    pub(crate) fn set_opacity(&self, opacity: f64) {
        // Glass owns the content; fading that view would also fade text and icons.
        if let PanelMaterial::Frosted { blur, .. } = &self.material
            && blur.alphaValue() != opacity
        {
            blur.setAlphaValue(opacity);
        }
    }

    pub(crate) fn blend_within_window(&self) {
        if let PanelMaterial::Frosted { blur, .. } = &self.material {
            blur.setBlendingMode(NSVisualEffectBlendingMode::WithinWindow);
        }
    }
}

fn frosted_backdrop(frame: NSRect, mtm: MainThreadMarker) -> Retained<NSVisualEffectView> {
    let backdrop = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), frame);
    backdrop.setMaterial(NSVisualEffectMaterial::Popover);
    backdrop.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    backdrop.setState(NSVisualEffectState::Active);
    let resize =
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable;
    backdrop.setAutoresizingMask(resize);
    // SAFETY: PanelSurface inherits NSView's frame initializer.
    let surface: Retained<PanelSurface> =
        unsafe { msg_send![PanelSurface::alloc(mtm), initWithFrame: backdrop.bounds()] };
    surface.setAutoresizingMask(resize);
    backdrop.addSubview(&surface);
    backdrop
}

pub(crate) fn tint(rgb: u32, alpha: f64) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(
        ((rgb >> 16) & 0xff) as f64 / 255.0,
        ((rgb >> 8) & 0xff) as f64 / 255.0,
        (rgb & 0xff) as f64 / 255.0,
        alpha,
    )
}
#[cfg(test)]
#[path = "../../../tests/native/material.rs"]
pub(crate) mod tests;
