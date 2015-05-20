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
use std::io::Write;
use std::io::stderr;

static RUNNING: AtomicBool = ATOMIC_BOOL_INIT;

fn main() {
    RUNNING.store(true, Ordering::SeqCst);
    CtrlC::set_handler(move || {
        RUNNING.store(false, Ordering::SeqCst);
    });
    let (w,r) = match bot::start("irc.quakenet.org", 6667) {
        Ok((w,r)) => (Some(w),Some(r)),
        Err(e) => {
            writeln!(stderr(), "{}", e).unwrap();
            (None,None)
        }
    };
    while RUNNING.load(Ordering::SeqCst) {}
    match (w, r) {
        (Some(w), Some(r)) => {
            w.join();
            r.join();
        },
        _ => {}
    }
}
