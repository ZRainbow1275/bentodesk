use super::renderer_is_stale;

#[test]
fn same_generation_is_not_stale() {
    assert!(!renderer_is_stale(0, 0));
    assert!(!renderer_is_stale(7, 7));
    assert!(!renderer_is_stale(u64::MAX, u64::MAX));
}

#[test]
fn changed_generation_is_stale() {
    // Generation only ever increases (one bump per recover_device_chain),
    // but the predicate is a plain inequality so direction is irrelevant.
    assert!(renderer_is_stale(0, 1));
    assert!(renderer_is_stale(3, 4));
    assert!(renderer_is_stale(1, 0));
}
