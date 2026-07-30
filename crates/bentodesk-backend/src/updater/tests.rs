use super::*;
use crossbeam_channel::unbounded;

const TEST_MINISIGN_PUBLIC_KEY: &str = "untrusted comment: minisign public key E7620F1842B4E81F\n\
     RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
const TEST_MINISIGN_SIGNATURE: &str = concat!(
    "untrusted comment: signature from minisign secret key\n",
    "RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/",
    "z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\n",
    "trusted comment: timestamp:1556193335\tfile:test\n",
    "y/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg=="
);
const TEST_MINISIGN_SIGNATURE_BASE64: &str = concat!(
    "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIG1pbmlzaWduIHNlY3JldCBrZXkKUlVRZjZMUkNHQTlp",
    "NTU5cjNnN1YxcU55SkRBcEdpcDhNZnFjYWRJZ1Q5Q3VoVjNFTWhIb04xbUdUa1VpZEYvejdTcmxRZ1hkeThvZmpi",
    "N2JOSkp5bERPb2NyQ284S0x6WndvPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNTU2MTkzMzM1CWZpbGU6",
    "dGVzdAp5L3JVdzJ5OC9oT1VZalpVNzFlSHAvV28xS1o0MGZHeTJWSkVEbDM0WE1KTStUWDQ4U3MvMTd1M0l2SWZi",
    "VlIxRmtaWlNOQ2lzUWJ1UVkrYkh3aEVCZz09"
);
const TEST_ARTIFACT_SHA256: &str =
    "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

fn updater_with_test_minisign_key(
    event_tx: crossbeam_channel::Sender<UpdateEvent>,
    manifest_path: &Path,
) -> Updater {
    Updater::with_manifest_source_and_minisign_key(
        event_tx,
        Some(SmolStr::new(manifest_path.to_string_lossy())),
        SmolStr::new_static(TEST_MINISIGN_PUBLIC_KEY),
    )
}

include!("tests/01_decode_tauri_minisign_signature_accepts_raw_and_base64.rs");
include!("tests/02_recurring_background_check_repeats_until_test_run_limit.rs");

#[test]
fn local_manifest_read_is_bounded() {
    let manifest_path = std::env::temp_dir().join(format!(
        "bentodesk-update-manifest-limit-{}.json",
        std::process::id()
    ));
    std::fs::write(&manifest_path, vec![b' '; MAX_MANIFEST_BYTES + 1]).expect("write manifest");
    let (tx, _rx) = unbounded::<UpdateEvent>();
    let updater =
        Updater::with_manifest_source(tx, Some(SmolStr::new(manifest_path.to_string_lossy())));

    assert!(matches!(
        updater.load_manifest_text(),
        Err(UpdaterError::FetchFailed(message)) if message.contains("exceeds")
    ));

    let _ = std::fs::remove_file(manifest_path);
}

#[test]
fn unsigned_update_artifacts_are_rejected() {
    let info = UpdateInfo {
        version: SmolStr::new_static("9.9.9"),
        current_version: SmolStr::new_static("0.0.1"),
        date: None,
        body: None,
        artifact_url: Some("https://example.invalid/BentoDesk.exe".to_owned()),
        artifact_sha256: None,
        signature: None,
    };

    assert!(matches!(
        validate_manifest_integrity_policy(&info),
        Err(UpdaterError::VerificationFailed(message)) if message.contains("must include")
    ));
}

#[test]
fn update_artifact_size_is_bounded() {
    assert!(validate_artifact_size(MAX_UPDATE_ARTIFACT_BYTES).is_ok());
    assert!(matches!(
        validate_artifact_size(MAX_UPDATE_ARTIFACT_BYTES + 1),
        Err(UpdaterError::FetchFailed(message)) if message.contains("exceeds")
    ));
}

#[test]
fn updater_http_transport_rejects_remote_plaintext() {
    let remote = parse_http_url("http://example.com/update.json").expect("parse");
    assert!(matches!(
        validate_http_transport(&remote, "http://example.com/update.json"),
        Err(UpdaterError::UnsupportedManifestSource(message))
            if message.contains("plaintext HTTP")
    ));

    let loopback = parse_http_url("http://127.0.0.1:8080/update.json").expect("parse");
    assert!(validate_http_transport(&loopback, "http://127.0.0.1:8080/update.json").is_ok());

    let secure = parse_http_url("https://example.com/update.json").expect("parse");
    assert!(validate_http_transport(&secure, "https://example.com/update.json").is_ok());
}
