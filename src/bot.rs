use ::timer::Timer;
use ::timer;
use std::fs::{create_dir, remove_file};
use std::io::{Result, BufRead, BufReader, Write};
use std::mem;
use std::net::Shutdown;
use std::net::TcpStream;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::thread;
use time::Duration;
use unix_socket::{UnixStream, UnixListener};

const SOCKETDIR: &'static str = "./sockets";
type PluginConnections = Mutex<Vec<BufReader<UnixStream>>>;

/// Represents a connection to one IRC server
pub struct Bot {
    threads: Vec<JoinHandle<()>>,
}
impl Bot {
    /// Creates a new connection to
    /// specified server and port.
    pub fn new<S: Into<String>>(server: S, port: u16) -> Result<Bot> {
        let server = server.into();
        let (plugin_tx, plugin_rx) = channel();
        let plugin_rx = Arc::new(Mutex::new(plugin_rx));

        let plugin_connections = listen_to_unix_socket(&server[..]);


        println!("Connecting to {}:{}", server, port);

        let ptx = plugin_tx.clone();
        let pluginconns = plugin_connections.clone();
        let reconnect_thread = thread::spawn(move || {
            let (connection_dead_tx, connection_dead_rx) = channel();
            if let Err(e) = Bot::connect(&server[..],
                                         port,
                                         ptx.clone(),
                                         plugin_rx.clone(),
                                         pluginconns.clone(),
                                         connection_dead_tx.clone()) {
                println!("{}", e);
                ::std::process::exit(1);
            }
            while super::RUNNING.load(Ordering::SeqCst) {
                match connection_dead_rx.try_recv() {
                    Ok(_) => {
                        Bot::connect(&server[..],
                                     port,
                                     ptx.clone(),
                                     plugin_rx.clone(),
                                     pluginconns.clone(),
                                     connection_dead_tx.clone());
                    }
                    Err(_) => {
                        thread::sleep(::std::time::Duration::from_secs(10));
                    }
                }
            }
        });

        // Plugin thread will listen to an unix socket for possible
        // commands from plugins. Currently the plugins should return
        // strings that are valid commands for IRC
        let plugin_thread = thread::spawn(move || {
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
                thread::sleep(::std::time::Duration::from_millis(250));
            }
        });

        Ok(Bot { threads: vec![reconnect_thread, plugin_thread] })
    }

    fn connect(server: &str,
               port: u16,
               plugin_tx: Sender<String>,
               plugin_rx: Arc<Mutex<Receiver<String>>>,
               plugin_connections: Arc<PluginConnections>,
               connection_dead_tx: Sender<()>)
               -> Result<(JoinHandle<()>, JoinHandle<()>)> {
        let stream = BufReader::new(try!(TcpStream::connect((&server[..], port))));
        let mut wstream = try!(stream.get_ref().try_clone());

        // Reader thread will read the TCP socket and
        // call handling of all lines without newlines
        let reader_thread = thread::spawn(move || {
            let connection_alive_timer = Timer::new();
            let mut guard = None;
            for full_line in stream.lines() {
                match full_line {
                    Ok(full_line) => {
                        for line in full_line.split("\r\n") {
                            handle_line(&plugin_connections, line);
                            if let Some(pong) = handle_pingpong(line,
                                                                &connection_alive_timer,
                                                                connection_dead_tx.clone(),
                                                                &mut guard) {
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
        let writer_thread = thread::spawn(move || {
            wstream.write(b"NICK RustBot\r\n").unwrap();
            wstream.write(b"USER RustBot 0 * :RustBot\r\n").unwrap();
            while super::RUNNING.load(Ordering::SeqCst) {
                match plugin_rx.lock().unwrap().try_recv() {
                    Ok(line) => {
                        wstream.write(line.as_bytes()).unwrap();
                        wstream.write(b"\r\n").unwrap();
                    }
                    Err(_) => {
                        thread::sleep(::std::time::Duration::from_millis(500));
                    }
                };
            }
            wstream.shutdown(Shutdown::Both).unwrap();
        });

        Ok((reader_thread, writer_thread))
    }

    pub fn wait_for_exit(self) {
        for thread in self.threads {
            thread.join().unwrap();
        }
    }
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
fn handle_pingpong(line: &str,
                   connection_alive_timer: &Timer,
                   connection_dead_tx: Sender<()>,
                   guard: &mut Option<timer::Guard>)
                   -> Option<String> {
    if line.starts_with("PING") {
        if guard.is_some() {
            let g = guard.take();
            drop(g); //Drop g here and cancel the timer
        }
        mem::replace(guard,
                     Some(connection_alive_timer.schedule_with_delay(Duration::minutes(5), move || {
                             let _ = connection_dead_tx.send(());
                         })));
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
                    client.set_read_timeout(Some(::std::time::Duration::from_millis(100))).unwrap();
                    let mut clients = clientdata.lock().unwrap();
                    clients.push(BufReader::new(client));
                }
                Err(err) => println!("New client failed, error: {}", err),
            }
        }
    });
    ret
}
