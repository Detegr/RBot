extern crate crossbeam;
extern crate ctrlc;
extern crate rustc_serialize;
extern crate time;
extern crate toml;
extern crate unix_socket;

mod config;
mod bot;

use std::sync::atomic::{ATOMIC_BOOL_INIT, AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

static RUNNING: AtomicBool = ATOMIC_BOOL_INIT;

fn main() {
    RUNNING.store(true, Ordering::SeqCst);
    ctrlc::set_handler(move || {
        RUNNING.store(false, Ordering::SeqCst);
    });
    let cfg = config::get(std::env::args()
        .nth(1)
        .unwrap_or("config.toml".to_owned())
        .as_str());
    bot::run(cfg);
}
