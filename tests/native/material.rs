use super::*;
use objc2_foundation::{NSSize, NSString};

pub fn verify_backdrop_opacity(backdrop: &PanelBackdrop, opacity: f64) {
    assert_eq!(backdrop.view().alphaValue(), 1.0);
    assert_eq!(backdrop.content.alphaValue(), 1.0);
    assert_eq!(backdrop.content.frame(), backdrop.view().bounds());
    match &backdrop.material {
        PanelMaterial::Glass(view) => {
            assert_eq!(view.contentView(), Some(backdrop.content.clone()));
            assert_eq!(view.style(), NSGlassEffectViewStyle::Regular);
            assert_eq!(view.cornerRadius(), 18.0);
            assert!(view.tintColor().is_none());
            assert!(backdrop.content.subviews().iter().all(|view| {
                view.downcast_ref::<NSVisualEffectView>().is_none()
                    && view.downcast_ref::<PanelSurface>().is_none()
            }));
        }
        PanelMaterial::Frosted { container, blur } => {
            assert_eq!(blur.alphaValue(), opacity);
            assert_eq!(blur.frame(), container.bounds());
            assert_eq!(blur.subviews().objectAtIndex(0).frame(), blur.bounds());
            assert_eq!(
                unsafe { backdrop.content.superview() },
                Some(container.clone())
            );
        }
    }
}

pub(crate) fn verify_panel_materials(mtm: MainThreadMarker) {
    for glass in [false, true] {
        if glass && !glass_available() {
            continue;
        }
        let backdrop = PanelBackdrop::with_glass(
            NSRect::new(NSPoint::ZERO, NSSize::new(700.0, 590.0)),
            mtm,
            glass,
        );
        let text = NSTextField::labelWithString(&NSString::from_str("Search"), mtm);
        backdrop.content.addSubview(&text);
        for appearance in unsafe { [NSAppearanceNameAqua, NSAppearanceNameDarkAqua] } {
            backdrop
                .view()
                .setAppearance(NSAppearance::appearanceNamed(appearance).as_deref());
            for height in [280.0, 590.0, 420.0] {
                backdrop.view().setFrameSize(NSSize::new(700.0, height));
                backdrop.view().layoutSubtreeIfNeeded();
                for opacity in [0.0, 0.5, 1.0] {
                    backdrop.set_opacity(opacity);
                    verify_backdrop_opacity(&backdrop, opacity);
                    assert_eq!(text.alphaValue(), 1.0);
                }
            }
        }
    }
    println!(
        "Panel materials: glass content, legacy opacity, resizing and light/dark appearances verified without showing windows."
    );
}
