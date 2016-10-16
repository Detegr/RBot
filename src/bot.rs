use ::config::Config;
use ::crossbeam;
use std::fs::{create_dir, remove_file};
use std::io::{BufRead, BufReader, Write};
use std::net::Shutdown;
use std::net::TcpStream;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use unix_socket::{UnixStream, UnixListener};

const SOCKETDIR: &'static str = "./sockets";
type PluginConnections = Mutex<Vec<BufReader<UnixStream>>>;

/// Creates a new connection to
/// specified server and port.
pub fn run(config: Config) {
    crossbeam::scope(|scope| {
        let (plugin_tx, plugin_rx) = channel();
        let plugin_rx = Arc::new(Mutex::new(plugin_rx));
        let plugin_connections = listen_to_unix_socket(&config.server);

        println!("Connecting to {}:{}", config.server, config.port);

        let ptx = plugin_tx.clone();
        let pluginconns = plugin_connections.clone();

        scope.spawn(move || {
            while super::RUNNING.load(Ordering::SeqCst) {
                connect(&config, ptx.clone(), plugin_rx.clone(), pluginconns.clone());
                println!("Reconnecting...");
                thread::sleep(Duration::from_secs(5));
            }
        });

        // Plugin thread will listen to an unix socket for possible
        // commands from plugins. Currently the plugins should return
        // strings that are valid commands for IRC
        scope.spawn(move || {
            let mut line = String::new();
            while super::RUNNING.load(Ordering::SeqCst) {
                for plugin_client in plugin_connections.lock().unwrap().iter_mut() {
                    match plugin_client.read_line(&mut line) {
                        Ok(_) => {
                            for line in line.lines() {
                                println!("Sending: {}", line);
                                let _ = plugin_tx.send(line.to_owned());
                            }
                            line.clear();
                        }
                        Err(_) => {
                            continue;
                        }
                    }
                }
                thread::sleep(Duration::from_millis(250));
            }
        });
    });
}

fn connect(config: &Config,
           plugin_tx: Sender<String>,
           plugin_rx: Arc<Mutex<Receiver<String>>>,
           plugin_connections: Arc<PluginConnections>) {

    crossbeam::scope(|scope| {
        let stream = BufReader::new(TcpStream::connect((&config.server[..], config.port)).unwrap());
        let mut wstream = stream.get_ref().try_clone().unwrap();

        // Reader thread will read the TCP socket and
        // call handling of all lines without newlines
        scope.spawn(move || {
            for full_line in stream.lines() {
                match full_line {
                    Ok(full_line) => {
                        for line in full_line.split("\r\n") {
                            handle_line(&plugin_connections, line);
                            if let Some(pong) = handle_pingpong(line) {
                                println!("{}", pong);
                                plugin_tx.send(pong).unwrap();
                            }
                        }
                    }
                    Err(e) => println!("{}", e),
                }
            }
        });

        // Writer thread will first write the initial lines needed
        // for a connection. Then it will start listening the
        // receiver end of a channel, waiting for input from reader_thread
        scope.spawn(move || {
            wstream.write(&format!("NICK {}\r\n", config.nick).as_bytes()).unwrap();
            wstream.write(&format!("USER {} 0 * :{}\r\n", config.nick, config.nick).as_bytes())
                .unwrap();
            while super::RUNNING.load(Ordering::SeqCst) {
                match plugin_rx.lock().unwrap().try_recv() {
                    Ok(line) => {
                        wstream.write(line.as_bytes()).unwrap();
                        wstream.write(b"\r\n").unwrap();
                    }
                    Err(_) => {
                        thread::sleep(Duration::from_millis(500));
                        match wstream.take_error() {
                            Ok(None) => {},
                            err => {
                                println!("{:?}", err);
                                break;
                            }
                        }
                    }
                };
            }
            wstream.shutdown(Shutdown::Both).unwrap();
        });
    });
}

/// Parses a line of text in IRC protocol format. The parsed line
/// is then restructured in a simpler format and sent to the unix
/// socket for possible plugin invocations.
fn handle_line(plugins: &Arc<PluginConnections>, line: &str) {
    for plugin in plugins.lock().unwrap().iter_mut() {
        let p = plugin.get_mut();
        match p.write(line.as_bytes())
            .and_then(|_| p.write(b"\r\n"))
            .and_then(|_| p.flush()) {
            Err(e) => println!("{}", e.to_string()),
            _ => {}
        }
    }
    println!("{}", line);
}

/// Handles a pingpong line, replacing PING with PONG
/// Resets a timer every time a PING is received to check if
/// the connection is still alive.
fn handle_pingpong(line: &str) -> Option<String> {
    if line.starts_with("PING") {
        Some(line.replace("PING", "PONG").to_owned())
    } else {
        None
    }
}

/// Opens up an unix socket and spawns a thread that will populate
/// the vector with the connections to the socket. Will probably
/// be renamed in the future
fn listen_to_unix_socket(socket: &str) -> Arc<PluginConnections> {
    let _ = create_dir(SOCKETDIR);
    let clientdata = Arc::new(Mutex::new(vec![]));
    let socketpath = format!("{}/{}", SOCKETDIR, socket);
    let _ = remove_file(&socketpath);
    println!("Creating a socket for '{}' to {}", socket, socketpath);
    let ret = clientdata.clone();
    thread::spawn(move || {
        let ulistener = match UnixListener::bind(&socketpath) {
            Ok(ulistener) => ulistener,
            Err(err) => panic!("Could not create UNIX socket to '{}': {}", socketpath, err),
        };
        for stream in ulistener.incoming() {
            match stream {
                Ok(client) => {
                    client.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
                    let mut clients = clientdata.lock().unwrap();
                    clients.push(BufReader::new(client));
                }
                Err(err) => println!("New client failed, error: {}", err),
            }
        }
    });
    ret
}
