use super::*;
use crate::macos::platform::quicklinks::destination_url;

pub fn verify_editor(mtm: MainThreadMarker) {
    use std::rc::Rc;
    let saved = Rc::new(RefCell::new(Vec::new()));
    let output = saved.clone();
    let editor = QuicklinkEditor::new(
        Vec::new(),
        Box::new(move |links| {
            winlane::core::config::Config {
                quicklinks: links.clone(),
                ..Default::default()
            }
            .validate()?;
            output.replace(links);
            Ok(())
        }),
        mtm,
    );
    editor.add(sel!(addQuicklink:), None);
    editor
        .ui()
        .name
        .setStringValue(&NSString::from_str("Search"));
    editor
        .ui()
        .link
        .setStringValue(&NSString::from_str("https://example.com/?q={Query}"));
    editor.input_changed();
    assert_eq!(saved.borrow().len(), 1);
    assert!(editor.ui().message.stringValue().is_empty());
    let shortcut = winlane::core::config::Shortcut {
        command: true,
        control: false,
        option: true,
        shift: false,
        key: "KeyG".into(),
    };
    editor.ui().shortcut.fill_optional(Some(&shortcut));
    editor.ui().shortcut.notify_changed();
    assert_eq!(saved.borrow()[0].shortcut.as_ref(), Some(&shortcut));
    editor
        .ui()
        .shortcut
        .fill_optional(Some(&Default::default()));
    editor.ui().shortcut.notify_changed();
    let error = editor.ui().message.stringValue().to_string();
    assert!(error.contains(tr!("快捷链接“Search”", "Quicklink “Search”")));
    assert!(error.contains(tr!("搜索模式（快捷键 1）", "Search mode (shortcut 1)")));
    assert!(error.contains("⌃I"));
    assert_eq!(editor.ui().message.toolTip().unwrap().to_string(), error);
    assert_eq!(
        saved.borrow()[0].shortcut.as_ref(),
        Some(&shortcut),
        "a conflict must preserve the active binding"
    );
    editor.ui().shortcut.fill_optional(None);
    editor.ui().shortcut.notify_changed();
    assert!(saved.borrow()[0].shortcut.is_none());
    assert!(editor.ui().message.stringValue().is_empty());
    assert!(editor.ui().message.toolTip().is_none());
    editor.ui().shortcut.fill_optional(Some(&shortcut));
    editor.ui().shortcut.notify_changed();
    editor
        .ui()
        .link
        .setStringValue(&NSString::from_str("https://example.com/{argument name="));
    editor.input_changed();
    assert_eq!(saved.borrow()[0].link, "https://example.com/?q={Query}");
    assert!(!editor.ui().message.stringValue().is_empty());
    let copy = QuicklinkEditor::new(Vec::new(), Box::new(|_| Ok(())), mtm);
    copy.restore_draft(editor.draft());
    assert_eq!(copy.ui().link.stringValue(), editor.ui().link.stringValue());
    assert_eq!(copy.ui().shortcut.read_optional().unwrap(), Some(shortcut));
    assert!(copy.view().window().is_none());
    let path = std::env::temp_dir().join(format!(
        "winlane-quicklink-test-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &path,
        r#"[{"name":"Docs","link":"https://example.test/docs"}]"#,
    )
    .unwrap();
    assert_eq!(editor.import_file(&path).unwrap(), (1, 0));
    assert_eq!(editor.import_file(&path).unwrap(), (0, 1));
    assert_eq!(saved.borrow().len(), 2);
    assert!(
        editor
            .ui()
            .link
            .stringValue()
            .to_string()
            .ends_with("name="),
        "import preserves unfinished draft"
    );
    std::fs::remove_file(path).unwrap();
    editor.delete(sel!(deleteQuicklink:), None);
    assert_eq!(saved.borrow().len(), 1);
    assert_eq!(saved.borrow()[0].name, "Docs");
    assert!(
        editor.ui().shortcut.read_optional().unwrap().is_none(),
        "selection must not inherit the previous link's shortcut"
    );
    let url = destination_url("https://example.com/?q=a%26b").unwrap();
    assert_eq!(
        url.absoluteString().unwrap().to_string(),
        "https://example.com/?q=a%26b"
    );
    let file = destination_url("~/Downloads/hello world.txt").unwrap();
    assert!(file.isFileURL());
    assert!(
        file.path()
            .unwrap()
            .to_string()
            .ends_with("/Downloads/hello world.txt")
    );
    assert!(
        file.absoluteString()
            .unwrap()
            .to_string()
            .ends_with("hello%20world.txt")
    );
    assert!(destination_url("javascript:alert(1)").is_err());
    println!(
        "Quicklink editor: autosave, invalid draft preservation, inline edits, import deduplication, delete."
    );
}
