#![feature(scoped)]
#![feature(buf_stream)]
#[macro_use]

extern crate nom;
extern crate ctrlc;
extern crate unix_socket;

mod bot;
mod parser;

use std::sync::atomic::{ATOMIC_BOOL_INIT, AtomicBool, Ordering};
use ctrlc::CtrlC;
use bot::Bot;

static RUNNING: AtomicBool = ATOMIC_BOOL_INIT;

fn main() {
    RUNNING.store(true, Ordering::SeqCst);
    CtrlC::set_handler(move || {
        RUNNING.store(false, Ordering::SeqCst);
    });
    Bot::new("irc.quakenet.org", 6667).unwrap();
    while RUNNING.load(Ordering::SeqCst) {}
}
