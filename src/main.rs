#![feature(scoped)]
#[macro_use]

extern crate nom;
extern crate ctrlc;

mod bot;
mod parser;

use std::sync::atomic::{ATOMIC_BOOL_INIT, AtomicBool, Ordering};
use ctrlc::CtrlC;

static RUNNING: AtomicBool = ATOMIC_BOOL_INIT;

fn main() {
    RUNNING.store(true, Ordering::SeqCst);
    CtrlC::set_handler(move || {
        RUNNING.store(false, Ordering::SeqCst);
    });
    let (w,r) = match bot::start("irc.quakenet.org", 6667) {
        Ok((w,r)) => (w,r),
        Err(e) => panic!("{}", e)
    };
    while RUNNING.load(Ordering::SeqCst) {
    }
    w.join();
    r.join();
}
