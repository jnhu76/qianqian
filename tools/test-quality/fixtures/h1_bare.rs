/// Fixture: bare-sleep shape (calibration negative control).
#[test]
fn fixture_bare_sleep_premise() {
    let parked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let leg = std::thread::spawn(move || { parked.store(true, std::sync::atomic::Ordering::SeqCst); });
    std::thread::sleep(std::time::Duration::from_millis(60));
    leg.join().expect("leg exits");
    assert!(parked.load(std::sync::atomic::Ordering::SeqCst));
}
