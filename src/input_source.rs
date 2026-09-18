use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};
use objc2_foundation::{MainThreadMarker, NSString, NSUserDefaults, ns_string};
use winlane::input_method::InputMethod;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn TISCopyCurrentKeyboardInputSource() -> CFTypeRef;
    fn TISCopyInputSourceForLanguage(language: CFStringRef) -> CFTypeRef;
    fn TISCreateInputSourceList(properties: CFDictionaryRef, include_all: u8) -> CFArrayRef;
    fn TISGetInputSourceProperty(source: CFTypeRef, key: CFStringRef) -> CFTypeRef;
    fn TISSelectInputSource(source: CFTypeRef) -> i32;
    static kTISPropertyInputSourceID: CFStringRef;
    static kTISPropertyInputSourceLanguages: CFStringRef;
    static kTISPropertyInputSourceIsSelectCapable: CFStringRef;
    static kTISPropertyInputSourceType: CFStringRef;
    static kTISTypeKeyboardLayout: CFStringRef;
}

#[derive(Clone)]
pub struct Source(CFType);

impl Source {
    pub fn current(_: MainThreadMarker) -> Option<Self> {
        // SAFETY: TIS is called on the main thread; Copy transfers ownership.
        Self::copied(unsafe { TISCopyCurrentKeyboardInputSource() })
    }

    fn copied(raw: CFTypeRef) -> Option<Self> {
        if raw.is_null() {
            None
        } else {
            // SAFETY: Callers pass only objects returned by TIS Copy functions.
            Some(Self(unsafe { CFType::wrap_under_create_rule(raw) }))
        }
    }

    pub fn for_language(language: &str, _: MainThreadMarker) -> Option<Self> {
        let language = CFString::new(language);
        // SAFETY: TIS accepts a live BCP 47 CFString and returns a retained source.
        Self::copied(unsafe { TISCopyInputSourceForLanguage(language.as_concrete_TypeRef()) })
    }

    pub fn by_id(id: &str, _: MainThreadMarker) -> Option<Self> {
        // SAFETY: This immutable framework constant is a live CFString.
        let key = unsafe { CFString::wrap_under_get_rule(kTISPropertyInputSourceID) };
        let properties = CFDictionary::from_CFType_pairs(&[(key, CFString::new(id))]);
        // SAFETY: Search only enabled sources with this exact ID, never all installed sources.
        let raw = unsafe { TISCreateInputSourceList(properties.as_concrete_TypeRef(), 0) };
        if raw.is_null() {
            return None;
        }
        // SAFETY: Create returns an owned array of live TIS source objects.
        let sources = unsafe { CFArray::<CFTypeRef>::wrap_under_create_rule(raw) };
        sources.iter().find_map(|raw| {
            // SAFETY: The array owns the source; retain it before the array is released.
            let source = Self(unsafe { CFType::wrap_under_get_rule(*raw) });
            let selectable = source
                .property(unsafe { kTISPropertyInputSourceIsSelectCapable })
                .and_then(|value| value.downcast_into::<CFBoolean>())
                .is_some_and(bool::from);
            selectable.then_some(source)
        })
    }

    fn property(&self, key: CFStringRef) -> Option<CFType> {
        // SAFETY: The retained source and framework property key remain live during the call.
        let raw = unsafe { TISGetInputSourceProperty(self.0.as_CFTypeRef(), key) };
        if raw.is_null() {
            None
        } else {
            // SAFETY: The source owns this CF property; the Get rule retains it.
            Some(unsafe { CFType::wrap_under_get_rule(raw) })
        }
    }

    pub fn id(&self) -> Option<String> {
        self.property(unsafe { kTISPropertyInputSourceID })
            .and_then(|value| value.downcast_into::<CFString>())
            .map(|value| value.to_string())
    }

    pub fn primary_language(&self) -> Option<String> {
        let languages = self
            .property(unsafe { kTISPropertyInputSourceLanguages })?
            .downcast_into::<CFArray>()?;
        // SAFETY: TIS owns an array of CF strings; retain the entry before
        // releasing the array and check its concrete type before using it.
        let language = unsafe { CFType::wrap_under_get_rule(*languages.get(0)?) }
            .downcast_into::<CFString>()?
            .to_string();
        (!language.is_empty()).then_some(language)
    }

    pub fn is_keyboard_layout(&self) -> bool {
        self.property(unsafe { kTISPropertyInputSourceType })
            .and_then(|value| value.downcast_into::<CFString>())
            .is_some_and(|kind| {
                // SAFETY: This immutable Carbon constant is a live CFString.
                kind == unsafe { CFString::wrap_under_get_rule(kTISTypeKeyboardLayout) }
            })
    }

    pub fn select(&self, mtm: MainThreadMarker) -> bool {
        if self
            .id()
            .is_some_and(|id| Self::current(mtm).and_then(|source| source.id()) == Some(id))
        {
            return true;
        }
        // SAFETY: The source is retained and selection occurs only on the main thread.
        unsafe { TISSelectInputSource(self.0.as_CFTypeRef()) == 0 }
    }
}

pub fn preferred(policy: InputMethod, mtm: MainThreadMarker) -> Option<Source> {
    match policy {
        InputMethod::Current => None,
        InputMethod::English => Source::for_language("en", mtm),
        InputMethod::Chinese => Source::for_language("zh", mtm),
        InputMethod::LastUsed => NSUserDefaults::standardUserDefaults()
            .stringForKey(ns_string!("WinlaneSearchInputSource"))
            .and_then(|id| Source::by_id(&id.to_string(), mtm)),
    }
}

pub fn remember(id: &str) {
    let defaults = NSUserDefaults::standardUserDefaults();
    let key = ns_string!("WinlaneSearchInputSource");
    if defaults
        .stringForKey(key)
        .is_some_and(|value| value.to_string() == id)
    {
        return;
    }
    // SAFETY: Store only a property-list string identifying the input source, never typed text.
    unsafe { defaults.setObject_forKey(Some(&NSString::from_str(id)), key) };
}
