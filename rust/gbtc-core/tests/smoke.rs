use gbtc_core::nonce_range_valid;

#[test]
fn smoke_nonce_range() {
    assert!(nonce_range_valid(0, 1));
    assert!(!nonce_range_valid(0, 0));
    assert!(nonce_range_valid(1, u32::MAX));
}
