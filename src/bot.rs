use std::net::{TcpStream};
use std::io::{BufStream,BufRead,Write};
use std::io::Result;
use std::thread;
use parser;
use parser::Command;

pub fn start(server: &str, port: u16) -> Result<()>
{
    let stream = BufStream::new(try!(TcpStream::connect((server, port))));
    let mut wstream = try!(stream.get_ref().try_clone());
    let mut lwstream = try!(stream.get_ref().try_clone()); // TODO: Use channel instead
    let listen_thread = thread::scoped(move || {
        for full_line in stream.lines() {
            for line in full_line.unwrap().split("\r\n") {
                let mut parsed = parser::parse_message(line.as_ref()).unwrap();
                println!("{}", line);
                println!("{:?}", parsed);
                if parsed.command == Command::Named("PING") {
                    parsed.command = Command::Named("PONG");
                    lwstream.write(format!("{}\r\n", parsed).as_bytes()).unwrap();
                }
            }
        }
    });
    wstream.write(b"NICK RustBot\r\n").unwrap();
    wstream.write(b"USER RustBot 0 * :RustBot\r\n").unwrap();
    listen_thread.join();
    Ok(())
}
