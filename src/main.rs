#[macro_use]
extern crate nom;

use std::str;
use std::fmt;
use std::net::ToSocketAddrs;
use std::io::prelude::*;
use std::io::BufReader; 
use std::net::TcpStream;

use nom::IResult;

#[derive(Debug)]
struct Message {
    prefix: Option<String>,
    command: String,
    parameters: Vec<String>,
}

impl Message {
    fn new(command: &str, parameters: &[&str]) -> Self {
        Message {
            prefix: None,
            command: command.to_owned(),
            parameters: parameters.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    fn serialize(&self) -> String {
        let mut res = String::new();

        if let Some(ref prefix) = self.prefix {
            res.push_str(prefix);
            res.push(' ');
        }
        
        res.push_str(&self.command);
        res.push(' ');
        
        let mut parameters = self.parameters.iter();
        
        {
            for param in (&mut parameters).take(self.parameters.len() - 1) {
                res.push_str(param);
                res.push(' ');
            }
        }
        
        if let Some(param) = parameters.next() {
            res.push(':');
            res.push_str(&param);
        }
        
        res
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.command, self.parameters.join(" "))
    }
}

named!(prefix<&str>, map_res!(delimited!(char!(':'), is_not!(" "), char!(' ')), str::from_utf8));
named!(command<&str>, map_res!(is_not!(" "), str::from_utf8));
named!(parameter, alt!(preceded!(char!(':'), nom::rest) | is_not!(" ")));
named!(parameters< Vec<&str> >, separated_list!(char!(' '), map_res!(parameter, str::from_utf8)));

named!(deserialize<Message>, chain!(
    pr: prefix? ~
    c: command ~
    char!(' ') ~
    pa: parameters
    , || { Message {
        prefix: pr.map(|p| p.to_owned()),
        command: c.to_owned(),
        parameters: pa.into_iter().map(|p| p.to_owned()).collect(),
    } } ));

struct IRCConnection {
    stream: TcpStream,
}

impl IRCConnection {
    fn connect<A: ToSocketAddrs>(addr: A) -> Self {
        let stream = TcpStream::connect(addr).unwrap();
        
        IRCConnection {
            stream: stream,
        }
    }
    
    fn messages(&self) -> Box<Iterator<Item=Message>> {
        let reader = BufReader::new(self.stream.try_clone().expect("Could not clone"));
        
        let iter = reader.lines().map(|result| {
            let line = result.expect("No Content");
            let result = deserialize(line.as_bytes());
            match result {
                IResult::Done(_, res) => res,
                IResult::Error(err) => panic!("{}", err),
                IResult::Incomplete(someting) => panic!("Incomplete: {:?}", someting),
            }
        });
        
        Box::new(iter)
    }
    
    fn send(&self, msg: &Message) {
        println!("> {}", msg);
        write!(&self.stream, "{}\r\n", msg.serialize()).expect("Could not write!");
    }
}

fn incoming(msg: Message, conn: &IRCConnection) {
    println!("< {}", msg);
    match msg.command.as_str() {
        "PING" => {
            let params:Vec<_> = msg.parameters.iter().map(|s| s.as_str()).collect();
            conn.send(&Message::new("PONG", &params));
        },
        _ => {}
    }
}

fn main() {
    let input = "chat.freenode.net:6667\nrhenium\nrhenium\nrhenium";
    let parts:Vec<_> = input.lines().collect();
    let conn = IRCConnection::connect(parts[0]);
    
    conn.send(&Message::new("NICK", &[parts[1]]));
    conn.send(&Message::new("USER", &[parts[2], "0", "*", parts[3]]));
    
    for msg in conn.messages() {
        incoming(msg, &conn);
    }
}