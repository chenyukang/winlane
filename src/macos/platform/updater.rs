use objc2::msg_send;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2_foundation::{MainThreadMarker, NSBundle, NSError, NSString, ns_string};

/// Owns Sparkle on the AppKit thread; its framework remains loaded for the process lifetime.
pub(crate) struct Updater {
    controller: Retained<AnyObject>,
    updater: Retained<AnyObject>,
    _main_thread: MainThreadMarker,
}

impl Updater {
    pub fn new(mtm: MainThreadMarker, delegate: &AnyObject) -> Result<Option<Self>, String> {
        let bundle = NSBundle::mainBundle();
        if bundle
            .objectForInfoDictionaryKey(ns_string!("SUFeedURL"))
            .is_none()
        {
            return Ok(None);
        }
        let frameworks = bundle
            .privateFrameworksPath()
            .ok_or("Missing Frameworks directory")?;
        let path = NSString::from_str(&format!("{frameworks}/Sparkle.framework"));
        let framework = NSBundle::bundleWithPath(&path).ok_or("Missing Sparkle.framework")?;
        // SAFETY: Only the framework inside our signed app bundle is loaded, never a search path.
        unsafe { framework.loadAndReturnError() }.map_err(|error| error.to_string())?;
        let class = AnyClass::get(c"SPUStandardUpdaterController")
            .ok_or("Missing SPUStandardUpdaterController")?;
        // SAFETY: These signatures are from Sparkle 2's public headers. AppKit owns the
        // delegate for the entire session; Sparkle references it weakly. No calls leave this thread.
        unsafe {
            let controller: Allocated<AnyObject> = msg_send![class, alloc];
            let controller: Retained<AnyObject> = msg_send![controller,
                initWithStartingUpdater: false,
                updaterDelegate: None::<&AnyObject>,
                userDriverDelegate: delegate,
            ];
            let updater: Retained<AnyObject> = msg_send![&controller, updater];
            let mut error: Option<Retained<NSError>> = None;
            let started: bool = msg_send![&updater, startUpdater: &mut error];
            if !started {
                return Err(
                    error.map_or_else(|| "Sparkle could not start".into(), |e| e.to_string())
                );
            }
            Ok(Some(Self {
                controller,
                updater,
                _main_thread: mtm,
            }))
        }
    }

    pub fn can_check(&self) -> bool {
        // SAFETY: SPUUpdater's public BOOL property, on its owning thread.
        unsafe { msg_send![&self.updater, canCheckForUpdates] }
    }

    pub fn check(&self) {
        if self.can_check() {
            // SAFETY: Public SPUStandardUpdaterController action with a nullable sender.
            unsafe {
                let _: () = msg_send![&self.controller, checkForUpdates: None::<&AnyObject>];
            }
        }
    }

    pub fn automatic_checks(&self) -> bool {
        // SAFETY: SPUUpdater's public BOOL property.
        unsafe { msg_send![&self.updater, automaticallyChecksForUpdates] }
    }

    pub fn set_automatic_checks(&self, enabled: bool) {
        // SAFETY: This public setter persists the preference and reschedules Sparkle's timer.
        unsafe {
            let _: () = msg_send![&self.updater, setAutomaticallyChecksForUpdates: enabled];
        }
    }
}

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../tests/native/updater.rs"]
pub(crate) mod tests;
