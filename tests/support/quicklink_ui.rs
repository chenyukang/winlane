pub fn verify_editor(mtm: MainThreadMarker) {
    use std::rc::Rc;
    let saved = Rc::new(RefCell::new(Vec::new()));
    let output = saved.clone();
    let editor = QuicklinkEditor::new(Vec::new(), Box::new(move |links| { output.replace(links); Ok(()) }), mtm);
    editor.add(sel!(addQuicklink:), None);
    editor.ui().name.setStringValue(&NSString::from_str("Search"));
    editor.ui().link.setStringValue(&NSString::from_str("https://example.com/?q={Query}"));
    editor.input_changed();
    assert_eq!(saved.borrow().len(),1);
    assert!(editor.ui().message.stringValue().is_empty());
    editor.ui().link.setStringValue(&NSString::from_str("https://example.com/{argument name="));
    editor.input_changed();
    assert_eq!(saved.borrow()[0].link,"https://example.com/?q={Query}");
    assert!(!editor.ui().message.stringValue().is_empty());
    let copy = QuicklinkEditor::new(Vec::new(), Box::new(|_| Ok(())), mtm);
    copy.copy_draft_from(&editor);
    assert_eq!(copy.ui().link.stringValue(),editor.ui().link.stringValue());
    assert!(copy.view().window().is_none());
    let path = std::env::temp_dir().join(format!("winlane-quicklink-test-{}.json",std::process::id()));
    std::fs::write(&path, r#"[{"name":"Docs","link":"https://example.test/docs"}]"#).unwrap();
    assert_eq!(editor.import_file(&path).unwrap(),(1,0));
    assert_eq!(editor.import_file(&path).unwrap(),(0,1));
    assert_eq!(saved.borrow().len(),2);
    assert!(editor.ui().link.stringValue().to_string().ends_with("name="),"import preserves unfinished draft");
    std::fs::remove_file(path).unwrap();
    editor.delete(sel!(deleteQuicklink:),None);
    assert_eq!(saved.borrow().len(),1);
    assert_eq!(saved.borrow()[0].name,"Docs");
    let url = destination_url("https://example.com/?q=a%26b").unwrap();
    assert_eq!(url.absoluteString().unwrap().to_string(), "https://example.com/?q=a%26b");
    let file = destination_url("~/Downloads/hello world.txt").unwrap();
    assert!(file.isFileURL());
    assert!(file.path().unwrap().to_string().ends_with("/Downloads/hello world.txt"));
    assert!(file.absoluteString().unwrap().to_string().ends_with("hello%20world.txt"));
    assert!(destination_url("javascript:alert(1)").is_err());
    println!("Quicklink editor: autosave, invalid draft preservation, inline edits, import deduplication, delete.");
}

