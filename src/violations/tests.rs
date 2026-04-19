use super::incremental_deepen_step;

#[test]
fn incremental_deepen_step_grows_exponentially_from_base_shift() {
    let first = incremental_deepen_step(10, 0);
    let second = incremental_deepen_step(10, 1);
    let third = incremental_deepen_step(10, 2);

    assert_eq!(first.ok(), Some(10));
    assert_eq!(second.ok(), Some(20));
    assert_eq!(third.ok(), Some(40));
}

#[test]
fn incremental_deepen_step_returns_error_on_overflow() {
    let value = incremental_deepen_step(u32::MAX, 1);
    assert!(value.is_err());
}
