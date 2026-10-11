/// Fixture: single-sample try_recv as detector.
#[test]
fn fixture_try_recv_sample() {
    let (tx, result) = std::sync::mpsc::channel::<()>();
    let _sealer = std::thread::spawn(move || tx.send(()).unwrap());
    assert!(
        matches!(result.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)),
        "seal overtook ack"
    );
}
