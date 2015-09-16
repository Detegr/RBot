use std::net::{TcpStream};
use std::io::{BufRead,BufReader,Write};
use std::io::Result;
use std::thread;
use std::sync::mpsc::{channel, Sender};
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::net::Shutdown;
use unix_socket::{UnixStream,UnixListener};
use std::sync::{Arc,Mutex};
use std::fs::{create_dir,remove_file};

const SOCKETDIR: &'static str = "./sockets";
type PluginConnections = Mutex<Vec<BufReader<UnixStream>>>;

/// Represents connection to one IRC server
pub struct Bot {
    threads: (JoinHandle<()>, JoinHandle<()>, JoinHandle<()>),
}
impl Bot {
    /// Creates a new connection to
    /// specified server and port.
    pub fn new(server: &str, port: u16) -> Result<Bot> {
        let (tx, rx) = channel();

        let plugin_connections = listen_to_unix_socket(server);
        println!("Connecting to {}:{}", server, port);

        let stream = BufReader::new(try!(TcpStream::connect((server, port))));
        let mut wstream = try!(stream.get_ref().try_clone());

        let rplugins = plugin_connections.clone();
        let rtx = tx.clone();

        // Reader thread will read the TCP socket and
        // call handling of all lines without newlines
        let reader_thread = thread::spawn(move || {
            for full_line in stream.lines() {
                match full_line {
                    Ok(full_line) => {
                        for line in full_line.split("\r\n") {
                            handle_line(&rplugins, line, &rtx);
                        }
                    }
                    Err(e) => println!("{}", e)
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
                match rx.try_recv() {
                    Ok(line) => {
                        wstream.write(line.as_bytes()).unwrap();
                        wstream.write(b"\r\n").unwrap();
                    }
                    Err(_) => {
                        thread::sleep_ms(500);
                    }
                };
            }
            wstream.shutdown(Shutdown::Both).unwrap();
        });

        // Plugin thread will listen to an unix socket for possible
        // commands from plugins. Currently the plugins should return
        // strings that are valid commands for IRC
        let plugin_thread = thread::spawn(move || {
            let mut line = String::new();
            let plugin_connections = plugin_connections.clone();
            while super::RUNNING.load(Ordering::SeqCst) {
                for plugin_client in plugin_connections.lock().unwrap().iter_mut() {
                    match plugin_client.read_line(&mut line) {
                        Ok(_) => {
                            for line in line.lines() {
                                println!("Sending: {}", line);
                                let _ = tx.send(line.to_owned());
                            }
                            line.clear();
                        },
                        Err(_) => {
                            continue;
                        }
                    }
                }
                thread::sleep_ms(250);
            }
        });

        Ok(Bot {
            threads: (reader_thread, writer_thread, plugin_thread)
        })
    }
    pub fn wait_for_exit(self) {
        self.threads.0.join().unwrap();
        self.threads.1.join().unwrap();
        self.threads.2.join().unwrap();
    }
}

/// Parses a line of text in IRC protocol format. The parsed line
/// is then restructured in a simpler format and sent to the unix
/// socket for possible plugin invocations. PING-PONG is handled here
/// because it's necessary for the connection and thus does not belong
/// in any particular plugin
fn handle_line(plugins: &Arc<PluginConnections>, line: &str, tx: &Sender<String>) {
    for plugin in plugins.lock().unwrap().iter_mut() {
        let p = plugin.get_mut();
        match p.write(line.as_bytes())
            .and_then(|_| p.write(b"\r\n"))
            .and_then(|_| p.flush()) {
            Err(e) => println!("{}", e.to_string()),
            _ => {},
        }
    }
    println!("{}", line);
    if let Some(pong) = handle_pingpong(line) {
        println!("{}", pong);
        tx.send(pong).unwrap();
    }
}

fn handle_pingpong(line: &str) -> Option<String> {
    match line.starts_with("PING") {
        true => Some(line.replace("PING", "PONG").to_owned()),
        false => None
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
            Ok(ulistener) => {
                ulistener
            },
            Err(err) => panic!("Could not create UNIX socket to '{}': {}", socketpath, err)
        };
        for stream in ulistener.incoming() {
            match stream {
                Ok(client) => {
                    client.set_read_timeout(Some(::std::time::Duration::from_millis(100))).unwrap();
                    let mut clients = clientdata.lock().unwrap();
                    clients.push(BufReader::new(client));
                }
                Err(err) => println!("New client failed, error: {}", err)
            }
        }
    });
    ret
}
