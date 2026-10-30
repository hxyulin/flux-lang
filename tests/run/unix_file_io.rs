fn main() {
    let path = std::env::temp_dir().join(format!("flux-file-{}", std::process::id()));
    std::fs::write(&path, b"linux and macos").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"linux and macos");
    let file = std::fs::File::open(&path).unwrap();
    assert_eq!(file.metadata().unwrap().len(), 15);
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 15);
    assert!(std::fs::metadata(&path).unwrap().is_file());
    drop(file);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap_err().kind(), std::io::ErrorKind::NotFound);
    println!("file reads, metadata and removal");
}
