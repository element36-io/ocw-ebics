#[macro_use]
extern crate lazy_static;

use std::sync::Mutex;
use std::thread::{spawn, JoinHandle};

use httpmock::standalone::start_standalone_server;
use tokio::task::LocalSet;

/// This is a standalone mock server that is used for testing
pub fn simulate_standalone_server() {
    let _ = STANDALONE_SERVER.lock().unwrap_or_else(|e| e.into_inner());
}

lazy_static! {
    static ref STANDALONE_SERVER: Mutex<JoinHandle<Result<(), String>>> = Mutex::new(spawn(|| {
        let srv = start_standalone_server(8081, false, None, false, usize::MAX);
        let mut runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        LocalSet::new().block_on(&mut runtime, srv)
    }));
}

