use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};
use objc2::rc::Retained;
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType, NSTextInputClient, NSTextView};
use objc2_foundation::{MainThreadMarker, NSArray, NSString, NSUserDefaults, ns_string};
use std::cell::RefCell;
use winlane::core::input_method::InputMethod;
use winlane::features::input_rules::{RestoreStrategy, Settings, SourceRule, WINLANE_ID};

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn TISCopyCurrentKeyboardInputSource() -> CFTypeRef;
    fn TISCopyInputSourceForLanguage(language: CFStringRef) -> CFTypeRef;
    fn TISCreateInputSourceList(properties: CFDictionaryRef, include_all: u8) -> CFArrayRef;
    fn TISGetInputSourceProperty(source: CFTypeRef, key: CFStringRef) -> CFTypeRef;
    fn TISSelectInputSource(source: CFTypeRef) -> i32;
    static kTISPropertyLocalizedName: CFStringRef;
    static kTISPropertyInputSourceCategory: CFStringRef;
    static kTISCategoryKeyboardInputSource: CFStringRef;
    static kTISNotifySelectedKeyboardInputSourceChanged: CFStringRef;
    static kTISNotifyEnabledKeyboardInputSourcesChanged: CFStringRef;
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

    pub fn description(&self) -> Option<winlane::features::input_indicator::InputSource> {
        let id = self.id()?;
        let name = self
            .property(unsafe { kTISPropertyLocalizedName })
            .and_then(|value| value.downcast_into::<CFString>())
            .map_or_else(|| id.clone(), |name| name.to_string());
        Some(winlane::features::input_indicator::InputSource {
            id,
            name,
            language: self.primary_language(),
        })
    }

    pub fn enabled(_: MainThreadMarker) -> Vec<winlane::features::input_indicator::InputSource> {
        // SAFETY: Carbon returns an owned array; false limits it to enabled sources.
        let raw = unsafe { TISCreateInputSourceList(std::ptr::null(), 0) };
        if raw.is_null() {
            return Vec::new();
        }
        let sources = unsafe { CFArray::<CFTypeRef>::wrap_under_create_rule(raw) };
        let mut result: Vec<_> = sources
            .iter()
            .filter_map(|raw| {
                let source = Self(unsafe { CFType::wrap_under_get_rule(*raw) });
                let selectable = source
                    .property(unsafe { kTISPropertyInputSourceIsSelectCapable })
                    .and_then(|v| v.downcast_into::<CFBoolean>())
                    .is_some_and(bool::from);
                let keyboard = source
                    .property(unsafe { kTISPropertyInputSourceCategory })
                    .and_then(|v| v.downcast_into::<CFString>())
                    .is_some_and(|v| {
                        v == unsafe {
                            CFString::wrap_under_get_rule(kTISCategoryKeyboardInputSource)
                        }
                    });
                (selectable && keyboard)
                    .then(|| source.description())
                    .flatten()
            })
            .collect();
        result.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        result.dedup_by(|a, b| a.id == b.id);
        result
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

thread_local! {
    static RULES: RefCell<Settings> = RefCell::new(Settings::default());
}

pub fn set_rules(rules: &Settings, _: MainThreadMarker) {
    RULES.with_borrow_mut(|current| *current = rules.clone());
}

#[cfg(test)]
pub fn set_policy(policy: InputMethod, mtm: MainThreadMarker) {
    set_rules(&Settings::for_winlane(policy), mtm);
}

pub fn policy(_: MainThreadMarker) -> InputMethod {
    RULES.with_borrow(Settings::winlane_policy)
}

pub fn resolve_rule(rule: &SourceRule, mtm: MainThreadMarker) -> Option<Source> {
    match rule {
        SourceRule::Source(source) => Source::by_id(&source.id, mtm),
        SourceRule::English => Source::for_language("en", mtm),
        SourceRule::Chinese => Source::for_language("zh", mtm),
        SourceRule::Global | SourceRule::Current => None,
    }
}

pub struct Preference {
    pub target: Option<Source>,
    pub locales: Option<Retained<NSArray<NSString>>>,
    pub layout: Option<String>,
}

impl Preference {
    pub fn configured(mtm: MainThreadMarker) -> Self {
        RULES.with_borrow(|rules| Self::for_rules(rules, mtm))
    }

    pub fn for_rules(rules: &Settings, mtm: MainThreadMarker) -> Self {
        let (source, restore) = rules.resolve(WINLANE_ID);
        let target = (restore == RestoreStrategy::LastUsed)
            .then(|| preferred(InputMethod::LastUsed, mtm))
            .flatten()
            .or_else(|| resolve_rule(source, mtm));
        Self::with_target(rules.winlane_policy(), target)
    }

    fn with_target(policy: InputMethod, target: Option<Source>) -> Self {
        let layout = target
            .as_ref()
            .filter(|source| {
                matches!(policy, InputMethod::English | InputMethod::LastUsed)
                    && source.is_keyboard_layout()
            })
            .and_then(Source::id);
        let language = target.as_ref().and_then(|source| match policy {
            InputMethod::English => Some("en".to_owned()),
            InputMethod::Chinese => Some("zh".to_owned()),
            InputMethod::LastUsed => source.primary_language(),
            InputMethod::Current => None,
        });
        Self {
            target,
            locales: language
                .map(|language| NSArray::from_slice(&[&*NSString::from_str(&language)])),
            layout,
        }
    }
}

pub fn align_editor(source: Option<&Source>, editor: &NSTextView) {
    if !NSTextInputClient::hasMarkedText(editor)
        && let Some(id) = source.and_then(Source::id)
        && let Some(context) = editor.inputContext()
        && context
            .selectedKeyboardInputSource()
            .is_none_or(|current| current.to_string() != id)
    {
        context.setSelectedKeyboardInputSource(Some(&NSString::from_str(&id)));
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

pub fn insert_keyboard_layout_text(editor: &NSTextView, event: &NSEvent) -> bool {
    if !editor.isEditable()
        || event.r#type() != NSEventType::KeyDown
        || NSTextInputClient::hasMarkedText(editor)
        || event.modifierFlags().intersects(
            NSEventModifierFlags::Command
                | NSEventModifierFlags::Control
                | NSEventModifierFlags::Option
                | NSEventModifierFlags::Function,
        )
        || event.characters().is_none_or(|text| text.is_empty())
    {
        return false;
    }
    // A third-party IME can still consume keyDown after TIS and the input
    // context both report ABC. Translate with the selected layout, then use
    // the editor's normal insertion path without dispatching to that old IME.
    let Some(text) = event.charactersByApplyingModifiers(event.modifierFlags()) else {
        return false;
    };
    let value = text.to_string();
    if value.is_empty() || !value.bytes().all(|byte| (b' '..=b'~').contains(&byte)) {
        return false;
    }
    // SAFETY: NSString is a supported text-input value; NSNotFound asks the
    // editor to replace its selection and preserves notifications and undo.
    unsafe {
        NSTextInputClient::insertText_replacementRange(
            editor,
            &text,
            objc2_foundation::NSRange::new(objc2_foundation::NSNotFound as usize, 0),
        );
    }
    true
}

pub fn selection_notification() -> Retained<NSString> {
    // SAFETY: The framework owns this immutable notification name.
    NSString::from_str(
        &unsafe { CFString::wrap_under_get_rule(kTISNotifySelectedKeyboardInputSourceChanged) }
            .to_string(),
    )
}

pub fn sources_notification() -> Retained<NSString> {
    // SAFETY: The framework owns this immutable notification name.
    NSString::from_str(
        &unsafe { CFString::wrap_under_get_rule(kTISNotifyEnabledKeyboardInputSourcesChanged) }
            .to_string(),
    )
}
