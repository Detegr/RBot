use std::net::{TcpStream};
use std::io::{BufRead,BufReader,Write};
use std::io::Result;
use std::thread;
use std::sync::mpsc::{channel, Sender};
use parser;
use parser::Command;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::net::Shutdown;
use unix_socket::{UnixStream,UnixListener};
use std::sync::{Arc,Mutex};
use std::fs::{create_dir,remove_file};

const SOCKETDIR: &'static str = "./sockets";
type Plugins = Mutex<Vec<BufReader<UnixStream>>>;

pub struct Bot {
    threads: (JoinHandle<()>, JoinHandle<()>, JoinHandle<()>),
}
impl Bot {
    pub fn new(server: &str, port: u16) -> Result<Bot> {
        let (tx, rx) = channel();

        let plugins = listen_to_unix_socket(server);
        println!("Connecting to {}:{}", server, port);

        let stream = BufReader::new(try!(TcpStream::connect((server, port))));
        let mut wstream = try!(stream.get_ref().try_clone());

        let rplugins = plugins.clone();
        let rtx = tx.clone();
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

        let plugin_thread = thread::spawn(move || {
            let mut line = String::new();
            let plugins = plugins.clone();
            while super::RUNNING.load(Ordering::SeqCst) {
                for plugin in plugins.lock().unwrap().iter_mut() {
                    match plugin.read_line(&mut line) {
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

fn handle_line(plugins: &Arc<Plugins>, line: &str, tx: &Sender<String>) {
    let mut parsed = match parser::parse_message(line.as_ref()) {
        Ok(line) => line,
        Err(e) => {
            println!("{}", e);
            return;
        }
    };
    {
        for plugin in plugins.lock().unwrap().iter_mut() {
            let p = plugin.get_mut();
            match p.write(parsed.to_whitespace_separated().as_bytes())
                .and_then(|_| p.write(b"\r\n"))
                .and_then(|_| p.flush()) {
                Err(e) => println!("{}", e.to_string()),
                _ => {},
            }
        }
    }
    if parsed.command == Command::Named("PING".into()) {
        parsed.command = Command::Named("PONG".into());
        let _ = tx.send(parsed.to_string());
    }
    println!("{:?}", parsed);
}

fn listen_to_unix_socket(socket: &str) -> Arc<Plugins> {
    let _ = create_dir(SOCKETDIR);
    let clientdata = Arc::new(Mutex::new(vec![]));
    let socketpath = format!("{}/{}", SOCKETDIR, socket);
    let _ = remove_file(&socketpath);
    println!("Creating status socket for '{}' to {}", socket, socketpath);
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
