use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::data::{CFData, CFDataCreateMutable};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use std::ptr;
use std::sync::Arc;
use winlane::clipboard::{ImageFormat, ImageInfo, MAX_IMAGE_BYTES};
use winlane::clipboard_assets::{SessionFiles, digest};
use winlane::tr;

#[link(name = "ImageIO", kind = "framework")]
unsafe extern "C" {
    fn CGImageSourceCreateWithData(data: CFTypeRef, options: CFTypeRef) -> CFTypeRef;
    fn CGImageSourceCopyPropertiesAtIndex(
        source: CFTypeRef,
        index: usize,
        options: CFTypeRef,
    ) -> core_foundation::dictionary::CFDictionaryRef;
    fn CGImageSourceCreateThumbnailAtIndex(
        source: CFTypeRef,
        index: usize,
        options: CFTypeRef,
    ) -> CFTypeRef;
    fn CGImageSourceGetType(source: CFTypeRef) -> CFStringRef;
    fn CGImageDestinationCreateWithData(
        data: *mut std::ffi::c_void,
        kind: CFStringRef,
        count: usize,
        options: CFTypeRef,
    ) -> CFTypeRef;
    fn CGImageDestinationAddImage(destination: CFTypeRef, image: CFTypeRef, properties: CFTypeRef);
    fn CGImageDestinationFinalize(destination: CFTypeRef) -> bool;
    static kCGImagePropertyPixelWidth: CFStringRef;
    static kCGImagePropertyPixelHeight: CFStringRef;
    static kCGImageSourceShouldCache: CFStringRef;
    static kCGImageSourceCreateThumbnailFromImageAlways: CFStringRef;
    static kCGImageSourceCreateThumbnailWithTransform: CFStringRef;
    static kCGImageSourceThumbnailMaxPixelSize: CFStringRef;
}
fn invalid() -> String {
    tr!(
        "图片无法读取或过大（最多 32 MB、1 亿像素）。",
        "Image is invalid or too large (maximum 32 MB and 100 megapixels)."
    )
    .into()
}
unsafe fn owned(value: CFTypeRef) -> Result<CFType, String> {
    if value.is_null() {
        Err(invalid())
    } else {
        Ok(unsafe { CFType::wrap_under_create_rule(value) })
    }
}
pub fn prepare(
    bytes: Vec<u8>,
    format: ImageFormat,
    files: &Arc<SessionFiles>,
) -> Result<ImageInfo, String> {
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
        return Err(invalid());
    }
    let key = digest(&bytes);
    let length = bytes.len();
    let data = CFData::from_arc(Arc::new(bytes));
    // ImageIO owns these CF objects; decoding is performed only by the background image worker.
    let options = unsafe {
        CFDictionary::from_CFType_pairs(&[(
            CFString::wrap_under_get_rule(kCGImageSourceShouldCache),
            CFBoolean::false_value().as_CFType(),
        )])
    };
    let source = unsafe {
        owned(CGImageSourceCreateWithData(
            data.as_CFTypeRef(),
            options.as_CFTypeRef(),
        ))?
    };
    let kind = unsafe { CGImageSourceGetType(source.as_CFTypeRef()) };
    if kind.is_null() || unsafe { CFString::wrap_under_get_rule(kind) } != format.pasteboard_type()
    {
        return Err(invalid());
    }
    let properties =
        unsafe { CGImageSourceCopyPropertiesAtIndex(source.as_CFTypeRef(), 0, ptr::null()) };
    if properties.is_null() {
        return Err(invalid());
    }
    let properties: CFDictionary<CFString, CFType> =
        unsafe { CFDictionary::wrap_under_create_rule(properties) };
    let dimension = |key| -> Result<u32, String> {
        let key = unsafe { CFString::wrap_under_get_rule(key) };
        properties
            .find(&key)
            .and_then(|value| value.downcast::<CFNumber>())
            .and_then(|value| value.to_i64())
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(invalid)
    };
    let mut info = ImageInfo {
        key,
        format,
        bytes: length,
        width: dimension(unsafe { kCGImagePropertyPixelWidth })?,
        height: dimension(unsafe { kCGImagePropertyPixelHeight })?,
        asset: None,
    };
    if !info.valid() {
        return Err(invalid());
    }
    let options = unsafe {
        CFDictionary::from_CFType_pairs(&[
            (
                CFString::wrap_under_get_rule(kCGImageSourceCreateThumbnailFromImageAlways),
                CFBoolean::true_value().as_CFType(),
            ),
            (
                CFString::wrap_under_get_rule(kCGImageSourceCreateThumbnailWithTransform),
                CFBoolean::true_value().as_CFType(),
            ),
            (
                CFString::wrap_under_get_rule(kCGImageSourceThumbnailMaxPixelSize),
                CFNumber::from(96_i32).as_CFType(),
            ),
            (
                CFString::wrap_under_get_rule(kCGImageSourceShouldCache),
                CFBoolean::false_value().as_CFType(),
            ),
        ])
    };
    let thumb = unsafe {
        owned(CGImageSourceCreateThumbnailAtIndex(
            source.as_CFTypeRef(),
            0,
            options.as_CFTypeRef(),
        ))?
    };
    let output = unsafe { CFDataCreateMutable(ptr::null(), 0) };
    if output.is_null() {
        return Err(invalid());
    }
    let output = unsafe { CFData::wrap_under_create_rule(output) };
    let png = CFString::new("public.png");
    let encoder = unsafe {
        owned(CGImageDestinationCreateWithData(
            output.as_CFTypeRef() as *mut _,
            png.as_concrete_TypeRef(),
            1,
            ptr::null(),
        ))?
    };
    unsafe {
        CGImageDestinationAddImage(encoder.as_CFTypeRef(), thumb.as_CFTypeRef(), ptr::null());
    }
    if !unsafe { CGImageDestinationFinalize(encoder.as_CFTypeRef()) } {
        return Err(invalid());
    }
    info.asset = Some(
        files
            .create(&info, data.bytes(), output.bytes())
            .map_err(|_| {
                tr!(
                    "无法保存剪贴板图片。",
                    "Could not save the clipboard image."
                )
            })?,
    );
    Ok(info)
}
