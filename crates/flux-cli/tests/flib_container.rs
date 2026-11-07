use flux_cli::flib::{Archive, Metadata, Section};

fn metadata() -> Metadata {
    Metadata {
        compiler: [42; 32],
        sections: (1..=9)
            .map(|tag| Section {
                tag,
                bytes: vec![tag as u8; tag as usize],
            })
            .collect(),
    }
}

#[test]
fn archive_and_metadata_round_trip() {
    let metadata = metadata();
    let archive = Archive {
        metadata: metadata.encode().unwrap(),
        object: Some(vec![0, 1, 2, 3, 4]),
    };
    let bytes = archive.encode().unwrap();
    assert_eq!(bytes, archive.encode().unwrap());
    let decoded = Archive::decode(&bytes).unwrap();
    assert_eq!(archive, decoded);
    assert_eq!(
        metadata,
        Metadata::decode(&decoded.metadata, [42; 32]).unwrap()
    );
    let metadata_only = Archive {
        metadata: archive.metadata,
        object: None,
    };
    assert_eq!(
        Archive::decode(&metadata_only.encode().unwrap()).unwrap(),
        metadata_only
    );
}

#[test]
fn incompatible_headers_and_malformed_sections_are_rejected() {
    let bytes = metadata().encode().unwrap();
    assert!(Metadata::decode(&bytes, [43; 32]).is_err());
    for size in 0..bytes.len() {
        assert!(
            Metadata::decode(&bytes[..size], [42; 32]).is_err(),
            "prefix {size}"
        );
    }
    let mut bad = bytes.clone();
    bad[8] = 99;
    assert!(Metadata::decode(&bad, [42; 32]).is_err());
    let mut bad = bytes.clone();
    bad[52] = 0;
    assert!(Metadata::decode(&bad, [42; 32]).is_err());
    let mut bad = bytes.clone();
    bad[56..64].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(Metadata::decode(&bad, [42; 32]).is_err());
    let mut bad = bytes.clone();
    let payload = bad.len() - 33;
    bad[payload] ^= 1;
    assert!(
        Metadata::decode(&bad, [42; 32])
            .unwrap_err()
            .to_string()
            .contains("checksum")
    );
    let mut bad = bytes;
    bad.push(0);
    assert!(Metadata::decode(&bad, [42; 32]).is_err());
}

#[test]
fn malformed_archives_are_rejected() {
    let archive = Archive {
        metadata: metadata().encode().unwrap(),
        object: Some(vec![1]),
    };
    let bytes = archive.encode().unwrap();
    for size in 0..68 {
        assert!(Archive::decode(&bytes[..size]).is_err(), "prefix {size}");
    }
    let mut bad = bytes.clone();
    bad[8] = b'/';
    assert!(Archive::decode(&bad).is_err());
    let mut bad = bytes.clone();
    bad[56..66].copy_from_slice(b"9999999999");
    assert!(Archive::decode(&bad).is_err());
    let mut bad = bytes.clone();
    bad.extend_from_slice(&bytes[8..]);
    assert!(Archive::decode(&bad).is_err());
    let mut bad = bytes;
    bad.pop();
    assert!(Archive::decode(&bad).is_err());
}

#[test]
fn malformed_directory_is_rejected_even_with_a_valid_checksum() {
    let bytes = metadata().encode().unwrap();
    for (at, replacement, expected) in [
        (52, 0u64.to_le_bytes(), "section tag"),
        (56, u64::MAX.to_le_bytes(), "section range"),
        (64, u64::MAX.to_le_bytes(), "section range"),
    ] {
        let mut bad = bytes.clone();
        bad[at..at + replacement.len()].copy_from_slice(&replacement);
        let end = bad.len() - 32;
        let checksum = flux_hir::flib::digest(&bad[..end]);
        bad[end..].copy_from_slice(&checksum);
        let error = Metadata::decode(&bad, [42; 32]).unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }
}
