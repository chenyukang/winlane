use super::*;

pub fn verify_dates(builder: &PlaceholderBuilder) {
    assert!(builder.mode.get() == BuilderMode::Date);
    assert_eq!(
        builder.preset.numberOfItems() as usize,
        DATE_PRESETS.len() + 1
    );
    for (index, preset) in DATE_PRESETS.iter().enumerate() {
        builder.preset.selectItemAtIndex(index as isize);
        builder.update();
        assert!(builder.insert.isEnabled());
        assert_eq!(builder.token().unwrap(), preset.token);
        assert!(!builder.preview.stringValue().is_empty());
    }
    let timestamp = render(
        &Template::parse("{timestamp}").unwrap(),
        "",
        &HashMap::new(),
    )
    .unwrap()
    .parse::<i64>()
    .unwrap();
    let now = objc2_foundation::NSDate::now().timeIntervalSince1970() as i64;
    assert!((now - timestamp).abs() <= 2);
    builder
        .preset
        .selectItemAtIndex(DATE_PRESETS.len() as isize);
    builder
        .format
        .setStringValue(ns_string!("yyyy-MM-dd 'at' HH:mm"));
    builder.update();
    assert!(!builder.format.isHidden());
    assert!(builder.insert.isEnabled());
    assert!(builder.preview.stringValue().to_string().contains(" at "));
    capture_builder(builder, "snippet-date-builder");
    builder.format.setStringValue(ns_string!(""));
    builder.update();
    assert!(!builder.insert.isEnabled());
}

pub fn verify_fields(builder: &PlaceholderBuilder) -> String {
    assert!(builder.mode.get() == BuilderMode::Field);
    assert!(!builder.insert.isEnabled());
    builder.reuse.selectItemAtIndex(1);
    builder.reuse();
    assert_eq!(builder.name.stringValue().to_string(), "Name");
    assert!(builder.insert.isEnabled());
    builder.reuse.selectItemAtIndex(0);
    builder.reuse();
    assert!(builder.name.stringValue().is_empty());
    assert!(!builder.insert.isEnabled());
    builder.name.setStringValue(ns_string!("Tone"));
    builder.kind.selectItemAtIndex(2);
    builder
        .options
        .setString(ns_string!("Friendly\nFormal\nChoose…"));
    builder.default.setString(ns_string!("Formal"));
    builder.update();
    assert!(!builder.options_scroll.isHidden());
    assert!(builder.insert.isEnabled());
    assert!(builder.preview.stringValue().to_string().contains("Formal"));
    capture_builder(builder, "snippet-field-builder");
    builder.default.setString(ns_string!("not in list"));
    builder.update();
    assert!(!builder.insert.isEnabled());
    builder.default.setString(ns_string!("Formal"));
    builder.update();
    builder.token().unwrap()
}

fn capture_builder(builder: &PlaceholderBuilder, name: &str) {
    if let Ok(directory) = std::env::var("WINLANE_PREVIEW_DIR") {
        let root = builder.view.window().unwrap().contentView().unwrap();
        root.layoutSubtreeIfNeeded();
        let bitmap = root
            .bitmapImageRepForCachingDisplayInRect(root.bounds())
            .unwrap();
        root.cacheDisplayInRect_toBitmapImageRep(root.bounds(), &bitmap);
        let png = unsafe {
            bitmap.representationUsingType_properties(
                NSBitmapImageFileType::PNG,
                &objc2_foundation::NSDictionary::new(),
            )
        }
        .unwrap();
        std::fs::create_dir_all(&directory).unwrap();
        assert!(png.writeToFile_atomically(
            &NSString::from_str(&format!("{directory}/{name}.png")),
            true
        ));
    }
}
