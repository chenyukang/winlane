use super::*;

pub fn verify_catalog() {
    let dir = std::env::temp_dir().join(format!("winlane-catalog-fixture-{}", std::process::id()));
    let first = dir.join("Applications");
    let second = dir.join("User Applications");
    let create = |root: &Path, name: &str, id: &str, background: bool| {
        let app = root.join(format!("{name}.app"));
        std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        std::fs::write(app.join("Contents/MacOS/fixture"), "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::write(app.join("Contents/Info.plist"), format!(r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>{id}</string><key>CFBundleName</key><string>{name}</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleExecutable</key><string>fixture</string><key>LSBackgroundOnly</key><{background}/></dict></plist>"#)).unwrap();
        app
    };
    let primary = create(&first, "Browser", "com.example.browser", false);
    let duplicate = create(&second, "Browser Copy", "com.example.browser", false);
    let helper = create(
        &primary.join("Contents/Helpers"),
        "Helper",
        "com.example.helper",
        false,
    );
    create(&first, "Background", "com.example.background", true);
    create(&first, ".Hidden", "com.example.hidden", false);
    create(
        &first.join("Utilities"),
        "Utility",
        "com.example.utility",
        false,
    );
    create(&first, "Winlane", "app.windowlane.desktop", false);
    let extra = create(&dir.join("Other"), "Another", "com.example.another", false);
    std::os::unix::fs::symlink(&first, first.join("loop")).unwrap();
    let result = collect(&[first, second], &[extra, duplicate, helper]);
    assert_eq!(result.len(), 3);
    assert_eq!(
        result
            .iter()
            .find(|app| app.target.bundle_id == "com.example.browser")
            .unwrap()
            .target
            .path,
        primary.to_str().unwrap()
    );
    assert!(
        result
            .iter()
            .any(|app| app.target.bundle_id == "com.example.utility")
    );
    assert!(
        result
            .iter()
            .any(|app| app.target.bundle_id == "com.example.another")
    );
    assert!(!usable_path(Path::new("/Volumes/Installer/Browser.app")));
    assert!(!usable_path(Path::new("/Users/example/.Trash/Browser.app")));
    assert!(!usable_path(Path::new(
        "/System/Library/CoreServices/AuthorizationPromptService.app"
    )));
    assert!(!usable_path(Path::new(
        "/Library/Application Support/Example/Updater.app"
    )));
    assert!(!usable_path(Path::new(
        "/Users/example/Library/Application Support/Example/Helper.app"
    )));
    assert!(usable_path(Path::new(
        "/System/Library/CoreServices/Finder.app"
    )));
    std::fs::remove_dir_all(dir).unwrap();
}
