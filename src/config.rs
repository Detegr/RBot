use ::toml;
use std::default::Default;
use std::fs::File;
use std::io::Read;

#[derive(RustcDecodable, Debug)]
pub struct Config {
    pub port: u16,
    pub nick: String,
    pub server: String,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            port: 6667,
            nick: "RustBot".to_string(),
            server: "irc.quakenet.org".to_string(),
        }
    }
}

pub fn get(path: &str) -> Config {
    let mut configfile = match File::open(path) {
        Ok(file) => file,
        Err(_) => {
            println!("Config file '{}' not found, using defaults.", path);
            return Default::default();
        }
    };

    let mut config_toml = String::new();
    configfile.read_to_string(&mut config_toml).unwrap();
    let parsed = toml::Parser::new(&config_toml).parse().unwrap();
    toml::decode(toml::Value::Table(parsed)).unwrap()
}
