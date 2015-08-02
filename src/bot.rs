use std::net::{TcpStream};
use bufstream::BufStream;
use std::io::{BufRead,Write};
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
use std::collections::HashMap;
use std::fs::{create_dir,remove_file};

const SOCKETDIR: &'static str = "./sockets";
const CHANNELS: &'static [&'static str] = &["#testchannel"]; // TODO: Read channels (and servers) from a config file

pub struct Bot {
    threads: (JoinHandle<()>, JoinHandle<()>),
}
impl Bot {
    pub fn new(server: &str, port: u16) -> Result<Bot> {
        let (tx, rx) = channel();

        let mut sockets = HashMap::new();
        sockets.insert(server.to_owned(), listen_to_unix_socket(server));
        println!("Connecting to {}:{}", server, port);

        let stream = BufStream::new(try!(TcpStream::connect((server, port))));
        let mut wstream = try!(stream.get_ref().try_clone());

        let server = server.to_owned();
        let reader_thread = thread::spawn(move || {
            for full_line in stream.lines() {
                for line in full_line.unwrap().split("\r\n") {
                    handle_line(&server[..], &mut sockets, line, &tx);
                }
            }
        });

        let writer_thread = thread::spawn(move || {
            wstream.write(b"NICK RustBot\r\n").unwrap();
            wstream.write(b"USER RustBot 0 * :RustBot\r\n").unwrap();
            while super::RUNNING.load(Ordering::SeqCst) {
                match rx.try_recv() {
                    Ok(line) => {wstream.write(line.as_bytes()).unwrap();}
                    Err(_) => {thread::sleep_ms(500);}
                };
            }
            wstream.shutdown(Shutdown::Both).unwrap();
        });

        Ok(Bot {
            threads: (reader_thread, writer_thread)
        })
    }
    pub fn wait_for_exit(self) {
        self.threads.0.join().unwrap();
        self.threads.1.join().unwrap();
    }
}

fn handle_line(server: &str, sockets: &mut HashMap<String, Arc<Mutex<Vec<UnixStream>>>>, line: &str, tx: &Sender<String>) {
    let mut parsed = parser::parse_message(line.as_ref()).unwrap();
    {
        let mut clients = sockets.get(server).unwrap().lock().unwrap();
        for mut client in &mut *clients {
            client.write(line.as_bytes()).unwrap();
        }
    }
    println!("{}", line);
    println!("{:?}", parsed);
    match parsed.command {
        Command::Named("PING") => {
            parsed.command = Command::Named("PONG");
            tx.send(format!("{}\r\n", parsed)).unwrap();
        },
        Command::Named("PRIVMSG") => {
            let destination = parsed.params[0];
            let key = destination.to_owned();
            println!("Retreiving value for '{}'", key);
            let socketdata = sockets.entry(key).or_insert_with(|| listen_to_unix_socket(destination));
            {
                let mut socket = socketdata.lock().unwrap();
                for mut client in &mut *socket {
                    client.write(line.as_bytes()).unwrap();
                }
            }
        },
        Command::Numeric(376) => { // End of MOTD
            for channel in CHANNELS {
                tx.send(format!("JOIN {}\r\n", channel)).unwrap();
            }
        },
        _ => {}
    }
}

fn listen_to_unix_socket(socket: &str) -> Arc<Mutex<Vec<UnixStream>>> {
    let _ = create_dir(SOCKETDIR);
    let clientdata = Arc::new(Mutex::new(vec![]));
    let socketpath = format!("{}/{}", SOCKETDIR, socket);
    let _ = remove_file(&socketpath);
    println!("Creating status socket for '{}' to {}", socket, socketpath);
    let ret = clientdata.clone();
    thread::spawn(move || {
        let ulistener = match UnixListener::bind(&socketpath) {
            Ok(ulistener) => ulistener,
            Err(err) => panic!("Could not create UNIX socket to '{}': {}", socketpath, err)
        };
        for stream in ulistener.incoming() {
            match stream {
                Ok(client) => {
                    let mut clients = clientdata.lock().unwrap();
                    clients.push(client);
                }
                Err(err) => println!("New client failed, error: {}", err)
            }
        }
    });
    ret
}
