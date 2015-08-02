#[macro_use]

extern crate nom;
extern crate ctrlc;
extern crate unix_socket;
extern crate bufstream;

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
    let bot = Bot::new("irc.quakenet.org", 6667).unwrap();
    while RUNNING.load(Ordering::SeqCst) {}
    bot.wait_for_exit();
}
