#[test]
fn fixture_oracle() {
    assert_eq!(events.snapshot(), vec![GateEvent::A, GateEvent::B]);
    assert!(wait_until(Duration::from_secs(5), || done()));
}
