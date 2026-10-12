/// Fixture: poll-shape sleep (throttle inside loop+condition+deadline).
#[test]
fn fixture_poll_throttle() {
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if events.lock().unwrap().contains(&"Engaged") { break true; }
        if std::time::Instant::now() >= deadline { break false; }
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    assert!(true);
}
