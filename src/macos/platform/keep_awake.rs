use core_foundation::{
    base::TCFType,
    string::{CFString, CFStringRef},
};
use std::time::SystemTime;
use winlane::{
    features::keep_awake::{Choice, IndicatorStatus},
    tr, trf,
};

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPMAssertionCreateWithDescription(
        kind: CFStringRef,
        name: CFStringRef,
        details: CFStringRef,
        reason: CFStringRef,
        path: CFStringRef,
        timeout: f64,
        action: CFStringRef,
        id: *mut u32,
    ) -> i32;
    fn IOPMAssertionRelease(id: u32) -> i32;
}

struct Assertion(u32);
impl Assertion {
    fn new(kind: &str, seconds: f64) -> Result<Self, String> {
        let kind = CFString::new(kind);
        let name = CFString::new("Winlane Keep Awake");
        let release = CFString::new("TimeoutActionRelease");
        let mut id = 0;
        // IOKit owns the assertion; the timeout also expires if the UI thread stalls.
        let result = unsafe {
            IOPMAssertionCreateWithDescription(
                kind.as_concrete_TypeRef(),
                name.as_concrete_TypeRef(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                seconds,
                release.as_concrete_TypeRef(),
                &mut id,
            )
        };
        if result != 0 {
            return Err(trf!(
                "无法开启防休眠（{}）。",
                "Could not enable Keep Awake ({}).",
                result
            ));
        }
        Ok(Self(id))
    }
}
impl Drop for Assertion {
    fn drop(&mut self) {
        // An already expired assertion may have been released by IOKit.
        unsafe {
            IOPMAssertionRelease(self.0);
        }
    }
}

struct Active {
    choice: Choice,
    deadline: Option<SystemTime>,
    _assertions: Vec<Assertion>,
}

#[derive(Default)]
pub struct KeepAwake {
    active: Option<Active>,
}
impl KeepAwake {
    pub fn apply(&mut self, choice: Choice) -> Result<(), String> {
        let Choice::Start { minutes, display } = choice else {
            self.active.take();
            return Ok(());
        };
        let now = SystemTime::now();
        let seconds = minutes.map_or(0.0, |m| f64::from(m) * 60.0);
        let mut assertions = vec![Assertion::new("PreventUserIdleSystemSleep", seconds)?];
        if display {
            assertions.push(Assertion::new("PreventUserIdleDisplaySleep", seconds)?);
        }
        // Acquire every new assertion before replacing the current session.
        self.active = Some(Active {
            choice,
            deadline: choice.deadline(now),
            _assertions: assertions,
        });
        Ok(())
    }

    pub fn expire(&mut self, now: SystemTime) -> bool {
        if self
            .active
            .as_ref()
            .and_then(|a| a.deadline)
            .is_some_and(|end| now >= end)
        {
            self.active.take();
            return true;
        }
        false
    }

    pub fn current_choice(&self, now: SystemTime) -> Choice {
        self.active
            .as_ref()
            .filter(|active| active.deadline.is_none_or(|end| now < end))
            .map_or(Choice::Stop, |active| active.choice)
    }

    pub fn status(&self) -> String {
        match &self.active {
            None => tr!("防休眠已关闭", "Keep Awake is off").into(),
            Some(active) => {
                let duration = active.deadline.map_or_else(
                    || tr!("直到手动关闭", "until turned off").into(),
                    |end| {
                        let remaining = end.duration_since(SystemTime::now()).unwrap_or_default();
                        let minutes = (remaining.as_secs()
                            + u64::from(remaining.subsec_nanos() > 0))
                        .div_ceil(60);
                        trf!("剩余 {} 分钟", "{} minutes remaining", minutes)
                    },
                );
                format!("{} · {}", active.choice.detail(), duration)
            }
        }
    }

    pub fn indicator_status(&self, now: SystemTime) -> Option<IndicatorStatus> {
        let active = self.active.as_ref()?;
        IndicatorStatus::new(active.choice, active.deadline, now)
    }
}

#[cfg(test)]
#[path = "../../../tests/native/keep_awake.rs"]
pub(crate) mod tests;
