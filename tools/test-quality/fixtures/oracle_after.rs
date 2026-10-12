#[test]
fn fixture_oracle() {
    assert!(events.snapshot().contains(&GateEvent::A));
    assert!(wait_until(Duration::from_secs(50), || done()));
}
