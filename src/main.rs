extern crate ctrlc;
extern crate time;
extern crate timer;
extern crate unix_socket;

mod bot;

use bot::Bot;
use std::sync::atomic::{ATOMIC_BOOL_INIT, AtomicBool, Ordering};
use std::thread;

static RUNNING: AtomicBool = ATOMIC_BOOL_INIT;

fn main() {
    RUNNING.store(true, Ordering::SeqCst);
    ctrlc::set_handler(move || {
        RUNNING.store(false, Ordering::SeqCst);
    });
    let bot = Bot::new("irc.quakenet.org", 6667).unwrap();
    while RUNNING.load(Ordering::SeqCst) {
        thread::sleep_ms(100);
    }
    bot.wait_for_exit();
}
