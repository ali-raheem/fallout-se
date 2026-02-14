use fallout_core::gender::Gender;

#[test]
fn maps_known_gender_values() {
    assert_eq!(Gender::from_raw(0), Gender::Male);
    assert_eq!(Gender::from_raw(1), Gender::Female);
}

#[test]
fn preserves_unknown_values() {
    assert_eq!(Gender::from_raw(2), Gender::Unknown(2));
    assert_eq!(Gender::from_raw(-1).raw(), -1);
}
