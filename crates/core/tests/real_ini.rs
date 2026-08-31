//! Exercises the ini editor against the real `OptiScaler.ini` shipped by the
//! mod, which is the file the app actually has to edit in place.

use opti_core::optiscaler::ini::{IniDocument, ValueType};

const REAL_INI: &str = include_str!("fixtures/OptiScaler.ini");

#[test]
fn round_trips_the_shipped_ini_byte_for_byte() {
    let doc = IniDocument::parse(REAL_INI);
    assert_eq!(
        doc.to_source(),
        REAL_INI,
        "parsing and reserializing must not change a single byte"
    );
}

#[test]
fn finds_every_section_and_key() {
    let doc = IniDocument::parse(REAL_INI);

    let sections = doc.section_names();
    assert!(
        sections.len() > 30,
        "expected the full section list, got {}",
        sections.len()
    );
    for expected in ["Upscalers", "FrameGen", "Menu", "Spoofing", "Log", "Hotfix"] {
        assert!(
            sections.iter().any(|s| s == expected),
            "missing [{expected}]"
        );
    }

    let keys = doc.keys();
    assert!(keys.len() > 250, "expected ~300 keys, got {}", keys.len());

    // Every key belongs to a section and carries its documentation.
    assert!(keys.iter().all(|key| !key.section.is_empty()));
    // Every key is documented, either by its own comment block or by the one
    // above the group of related keys it belongs to.
    let undocumented: Vec<&str> = keys
        .iter()
        .filter(|key| key.help.is_empty())
        .map(|key| key.key.as_str())
        .collect();
    assert!(
        undocumented.is_empty(),
        "keys with no help text: {undocumented:?}"
    );
}

#[test]
fn classifies_known_keys_correctly() {
    let doc = IniDocument::parse(REAL_INI);
    let keys = doc.keys();
    let find = |section: &str, key: &str| {
        keys.iter()
            .find(|k| k.section == section && k.key == key)
            .unwrap_or_else(|| panic!("missing {section}.{key}"))
    };

    let dx11 = find("Upscalers", "Dx11Upscaler");
    let ValueType::Enum(options) = &dx11.value_type else {
        panic!("Dx11Upscaler should be an enum, got {:?}", dx11.value_type);
    };
    for expected in ["fsr22", "xess", "dlss"] {
        assert!(options.iter().any(|o| o == expected), "missing {expected}");
    }

    assert_eq!(find("FrameGen", "Enabled").value_type, ValueType::Bool);
    assert_eq!(
        find("Framerate", "FramerateLimit").value_type,
        ValueType::Float
    );

    // Everything ships defaulted to `auto`.
    assert!(dx11.is_default());
}

#[test]
fn edits_only_the_line_that_changed() {
    let mut doc = IniDocument::parse(REAL_INI);
    assert!(doc.set("FrameGen", "Enabled", "true"));
    assert!(doc.set("Upscalers", "Dx12Upscaler", "xess"));

    let edited = doc.to_source();
    assert_eq!(doc.get("FrameGen", "Enabled"), Some("true"));
    assert_eq!(doc.get("Upscalers", "Dx12Upscaler"), Some("xess"));

    // Same number of lines, and only the two values differ.
    assert_eq!(edited.lines().count(), REAL_INI.lines().count());
    let changed = REAL_INI
        .lines()
        .zip(edited.lines())
        .filter(|(before, after)| before != after)
        .count();
    assert_eq!(changed, 2, "exactly two lines should differ");
}
