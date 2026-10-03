// Nix builds have no .git, so build.rs formats SOURCE_DATE_EPOCH by hand
#[allow(dead_code)]
#[path = "../build.rs"]
mod build;

proptest::proptest! {
    #[test]
    fn formats_any_epoch_as_a_utc_date(epoch in -2_208_988_800i64..=253_402_300_799) {
        let timestamp = jiff::Timestamp::from_second(epoch).unwrap();
        let expected = jiff::fmt::strtime::format("%Y-%m-%dT%H:%M:%SZ", timestamp).unwrap();

        proptest::prop_assert_eq!(build::format_utc(epoch), expected);
    }
}
