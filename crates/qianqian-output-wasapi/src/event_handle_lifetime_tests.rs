//! Event-handle lifetime regressions (NATIVE-BOUNDARY-AUDIT-0 A3.3
//! corrective; round record: Git history / PR #134): the buffer event must have exactly one CloseHandle per
//! successful CreateEventW, on every exit path.
//!
//! Windows-only by nature — the mechanism does not exist elsewhere. The
//! leak loop opens the real default endpoint; it belongs to the Windows
//! reality gate, not to the platform-independent suites.

#[cfg(windows)]
mod windows {
    use std::sync::{Arc, Condvar, Mutex};

    use crate::wasapi::WasapiOutput;
    use qianqian_audio_api::ports::{
        AudioOutput, PcmFormat, PcmPull, RenderGate, RenderPcmInput, RenderRequest,
    };
    use windows::Win32::Foundation::{WAIT_FAILED, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{
        CreateEventW, GetCurrentProcess, GetProcessHandleCount, WaitForSingleObject,
    };

    const TEST_FORMAT: PcmFormat = PcmFormat {
        sample_rate: 44100,
        channels: 2,
        channel_mask: 0x3,
    };

    /// Positive control for the leak oracle: a freshly created event is a
    /// live handle, and an explicitly closed one is observably dead. This
    /// is what makes the leak-loop assertion below meaningful rather than
    /// vacuous.
    #[test]
    fn a_closed_event_is_observable_as_invalid() {
        unsafe {
            let event = CreateEventW(None, false, false, None).expect("CreateEventW");
            assert_eq!(
                WaitForSingleObject(event, 0),
                WAIT_TIMEOUT,
                "a live nonsignaled event times out, it does not fail"
            );
            let _ = windows::Win32::Foundation::CloseHandle(event);
            assert_eq!(
                WaitForSingleObject(event, 0),
                WAIT_FAILED,
                "a closed handle must fail, not time out like a live one"
            );
        }
    }

    /// The render input the cycles consume. `Eof` ends the episode by
    /// natural drain; `Block` parks the render thread in read_frames so
    /// stop_and_join exercises the stop path.
    enum Feed {
        Eof,
        Block(Arc<Parker>),
    }

    struct Parker {
        stopped: Mutex<bool>,
        cv: Condvar,
    }

    struct TestInput {
        feed: Feed,
    }

    impl RenderPcmInput for TestInput {
        fn read_frames(&self, dst: &mut [f32]) -> PcmPull {
            match &self.feed {
                Feed::Eof => {
                    dst.fill(0.0);
                    PcmPull::Eof
                }
                Feed::Block(parker) => {
                    let mut stopped = parker.stopped.lock().expect("parker lock");
                    while !*stopped {
                        stopped = parker.cv.wait(stopped).expect("parker wait");
                    }
                    PcmPull::Stopped
                }
            }
        }

        fn stop(&self) {
            if let Feed::Block(parker) = &self.feed {
                let mut stopped = parker.stopped.lock().expect("parker lock");
                *stopped = true;
                parker.cv.notify_all();
            }
        }
    }

    fn open_and_close_cycle(blocking: bool) {
        let output = WasapiOutput::new().expect("mechanism binds");
        let (feed, _parker) = if blocking {
            let parker = Arc::new(Parker {
                stopped: Mutex::new(false),
                cv: Condvar::new(),
            });
            (Feed::Block(parker.clone()), Some(parker))
        } else {
            (Feed::Eof, None)
        };
        let stream = output
            .open_stream(RenderRequest {
                format: TEST_FORMAT,
                input: Arc::new(TestInput { feed }),
                drain: Default::default(),
                // No pause is routed in these cycles; the mechanism must
                // sail through the loop-top gate untouched.
                gate: RenderGate::new(),
                // The real leg publishes position evidence into this
                // cell; nothing reads it in this oracle.
                position: Default::default(),
            })
            .expect("real endpoint opens");
        stream.stop_and_join();
    }

    fn process_handle_count() -> u32 {
        unsafe {
            let mut count = 0u32;
            GetProcessHandleCount(GetCurrentProcess(), &mut count).expect("GetProcessHandleCount");
            count
        }
    }

    /// The leak loop: alternating drain and stop cycles over the real
    /// endpoint. Before the EventHandle guard, every successful episode
    /// leaked one kernel event handle (a plain `HANDLE` field has no
    /// Drop), so N cycles grew the process handle count by ~N. With the
    /// guard the count returns to its baseline within a small tolerance
    /// for transient COM internals.
    #[test]
    fn repeated_open_stop_cycles_do_not_leak_event_handles() {
        const CYCLES: u32 = 30;
        const TOLERANCE: i64 = 4;

        // Warm-up: one cycle each way so first-touch allocations (DLLs,
        // COM apartment caches) are not mistaken for a leak.
        open_and_close_cycle(false);
        open_and_close_cycle(true);

        let baseline = process_handle_count();
        for cycle in 0..CYCLES {
            open_and_close_cycle(cycle % 2 == 1);
        }
        let after = process_handle_count();
        let growth = i64::from(after) - i64::from(baseline);
        assert!(
            growth <= TOLERANCE,
            "{CYCLES} open/close cycles grew the handle count by {growth}; \
             a per-episode event-handle leak is ~1 handle per cycle"
        );
    }
}
