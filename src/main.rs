extern crate rustc_serialize;
extern crate ctrlc;
extern crate time;
extern crate timer;
extern crate unix_socket;
extern crate toml;

mod bot;

use bot::Bot;
use std::sync::atomic::{ATOMIC_BOOL_INIT, AtomicBool, Ordering};
use std::thread;
use std::fs::File;
use std::io::Read;

static RUNNING: AtomicBool = ATOMIC_BOOL_INIT;

fn get_config(path: &str) -> bot::Config {
    let mut configfile = match File::open(path) {
        Ok(file) => file,
        Err(_) => {
            println!("Config file '{}' not found, using defaults.", path);
            return bot::Config::new();
        }
    };

    let mut config_toml = String::new();
    configfile.read_to_string(&mut config_toml).unwrap();
    let parsed = toml::Parser::new(&config_toml).parse().unwrap();
    return toml::decode(toml::Value::Table(parsed)).unwrap();
}

fn main() {
    RUNNING.store(true, Ordering::SeqCst);
    ctrlc::set_handler(move || {
        RUNNING.store(false, Ordering::SeqCst);
    });
    let cfg = get_config(std::env::args().nth(1)
                         .unwrap_or("config.toml".to_string())
                         .as_str());
    let bot = Bot::new(cfg).unwrap();
    while RUNNING.load(Ordering::SeqCst) {
        thread::sleep_ms(100);
    }
    bot.wait_for_exit();
}
