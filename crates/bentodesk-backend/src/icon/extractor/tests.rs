use super::*;

#[test]
fn compute_icon_hash_deterministic() {
    let hash1 = compute_icon_hash("C:\\Users\\test\\Desktop\\file.txt");
    let hash2 = compute_icon_hash("C:\\Users\\test\\Desktop\\file.txt");
    assert_eq!(hash1, hash2);
}

#[test]
fn compute_icon_hash_different_paths_differ() {
    let hash1 = compute_icon_hash("C:\\file_a.txt");
    let hash2 = compute_icon_hash("C:\\file_b.txt");
    assert_ne!(hash1, hash2);
}

#[test]
fn compute_icon_hash_is_hex_string() {
    let hash = compute_icon_hash("test_path");
    assert_eq!(hash.len(), 16);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn resolve_lnk_target_returns_none_for_non_lnk() {
    let r = resolve_lnk_target("C:/does-not-exist.txt");
    assert!(r.is_none());
}

#[test]
fn internet_shortcut_parser_reads_explorer_icon_resource() {
    let parsed = parse_internet_shortcut_icon(
        "[{000214A0-0000-0000-C000-000000000046}]\r\n\
         Prop3=19,0\r\n\
         [InternetShortcut]\r\n\
         IDList=\r\n\
         IconIndex=3\r\n\
         URL=steam://rungameid/843380\r\n\
         IconFile=D:\\Steam\\steam\\games\\fox.ico\r\n",
    )
    .expect("icon resource");

    assert_eq!(parsed.path, "D:\\Steam\\steam\\games\\fox.ico");
    assert_eq!(parsed.index, 3);
}

#[test]
fn internet_shortcut_parser_is_case_insensitive_and_unquotes_path() {
    let parsed = parse_internet_shortcut_icon(
        "[internetshortcut]\niconfile=\"C:\\Icons\\game.dll\"\niconindex=-42\n",
    )
    .expect("quoted icon resource");

    assert_eq!(parsed.path, "C:\\Icons\\game.dll");
    assert_eq!(parsed.index, -42);
}

#[test]
fn internet_shortcut_parser_ignores_icon_outside_target_section() {
    assert!(parse_internet_shortcut_icon("[Other]\nIconFile=C:\\wrong.ico\n").is_none());
}

#[test]
fn internet_shortcut_decoder_accepts_utf16le_bom() {
    let source = "[InternetShortcut]\r\nIconFile=C:\\Icons\\fox.ico\r\n";
    let mut bytes = vec![0xff, 0xfe];
    for unit in source.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }

    let decoded = decode_internet_shortcut_text(&bytes).expect("UTF-16LE shortcut");
    assert_eq!(
        parse_internet_shortcut_icon(&decoded)
            .expect("decoded icon")
            .path,
        "C:\\Icons\\fox.ico"
    );
}

#[test]
fn internet_shortcut_file_read_is_bounded() {
    let path = std::env::temp_dir().join(format!("bentodesk-url-limit-{}.url", std::process::id()));
    let mut content = b"[InternetShortcut]\nIconFile=C:\\Icons\\fox.ico\n".to_vec();
    content.resize(MAX_INTERNET_SHORTCUT_BYTES + 1, b' ');
    std::fs::write(&path, content).expect("write shortcut");

    assert!(read_internet_shortcut_icon(&path.to_string_lossy()).is_none());

    let _ = std::fs::remove_file(path);
}

#[test]
fn automatic_icon_extraction_rejects_network_paths_before_shell_access() {
    for path in [
        r"\\server\share\app.exe",
        r"\\?\UNC\server\share\shortcut.lnk",
    ] {
        assert!(matches!(
            extract_icon_png(path),
            Err(IconError::Io { message, .. }) if message.contains("network paths")
        ));
    }
}

#[test]
fn premultiplied_bgra_becomes_straight_rgba() {
    let mut pixels = [16, 32, 64, 128, 0, 0, 0, 0];

    normalize_color_bgra_to_rgba(&mut pixels, None, true).expect("normalise pixels");

    assert_eq!(pixels, [128, 64, 32, 128, 0, 0, 0, 0]);
}

#[test]
fn straight_bgra_is_not_unpremultiplied_twice() {
    let mut pixels = [200, 100, 50, 128];

    normalize_color_bgra_to_rgba(&mut pixels, None, false).expect("normalise pixels");

    assert_eq!(pixels, [50, 100, 200, 128]);
}

#[test]
fn dark_straight_bgra_is_not_mistaken_for_premultiplied_data() {
    let mut pixels = [32, 24, 16, 128];

    normalize_color_bgra_to_rgba(&mut pixels, None, false).expect("normalise pixels");

    assert_eq!(pixels, [16, 24, 32, 128]);
}

#[test]
fn zero_alpha_colour_icon_uses_and_mask_opacity() {
    let mut pixels = [30, 20, 10, 0, 60, 50, 40, 0];
    let mask = [0, 0, 0, 0, 255, 255, 255, 0];

    normalize_color_bgra_to_rgba(&mut pixels, Some(&mask), false).expect("normalise pixels");

    assert_eq!(pixels, [10, 20, 30, 255, 40, 50, 60, 0]);
}

#[test]
fn monochrome_hicon_combines_and_and_xor_planes() {
    let mask = [
        0, 0, 0, 0, 255, 255, 255, 0, // AND: opaque, transparent
        255, 255, 255, 0, 0, 0, 0, 0, // XOR: white, black
    ];

    let (pixels, invert_mask) = monochrome_mask_to_rgba(&mask, 2, 1).expect("monochrome icon");

    assert_eq!(pixels, [255, 255, 255, 255, 0, 0, 0, 0]);
    assert!(invert_mask.is_none());
}

#[test]
fn monochrome_hicon_keeps_background_inversion_visible() {
    let mask = [
        255, 255, 255, 0, // AND: invert/transparent bit
        255, 255, 255, 0, // XOR: invert bit
    ];

    let (pixels, invert_mask) = monochrome_mask_to_rgba(&mask, 1, 1).expect("monochrome icon");
    assert_eq!(pixels, [255, 255, 255, 0]);
    assert_eq!(invert_mask.as_deref(), Some(&[1][..]));
}

#[test]
fn legacy_invert_mask_stays_in_a_valid_png_chunk() {
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};

    // SAFETY: balance only the apartment initialised by this test thread.
    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let mut png = wic::encode_png(&[255, 255, 255, 0], 1, 1).expect("encode PNG");
    crate::icon::hicon::attach_legacy_invert_mask(&mut png, 1, 1, &[1])
        .expect("attach invert mask");

    let (width, height, bits) = crate::icon::legacy_invert_mask(&png).expect("invert mask");
    assert_eq!((width, height, bits), (1, 1, &[1][..]));
    assert!(wic::decode_png_alpha_check(&png));
    assert!(crate::icon::hicon::png_has_visible_content(&png));
    if initialized {
        // SAFETY: balances this thread's successful CoInitializeEx.
        unsafe { CoUninitialize() };
    }
}

#[test]
fn icon_pixel_length_preserves_non_square_dimensions() {
    assert_eq!(checked_icon_pixel_len(2, 3), Ok(24));
    assert!(checked_icon_pixel_len(0, 3).is_err());
    assert!(checked_icon_pixel_len(MAX_ICON_DIMENSION + 1, 1).is_err());
}

#[test]
fn extracts_real_windows_resource_icon() {
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};

    // SAFETY: balance only an apartment initialised by this test thread.
    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let path = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"))
        .join("System32")
        .join("shell32.dll");
    let result = extract_icon_via_extract_icon_ex(&path.to_string_lossy(), 0);
    let transparent = result
        .as_ref()
        .map(|png| wic::decode_png_alpha_check(png))
        .unwrap_or(true);
    if initialized {
        // SAFETY: balances this thread's successful CoInitializeEx after the
        // extraction and WIC alpha check released every COM object.
        unsafe { CoUninitialize() };
    }

    let png = result.expect("extract shell32 icon");
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(!transparent);
}
