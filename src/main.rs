#![feature(scoped)]
#[macro_use]

extern crate nom;

mod bot;
mod parser;

fn main() {
    match bot::start("irc.quakenet.org", 6667) {
        Ok(_) => {},
        Err(e) => println!("{}", e)
    }
}
