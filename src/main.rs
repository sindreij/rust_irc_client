extern crate regex;
#[macro_use]
extern crate nom;


use nom::{IResult,digit};
use nom::IResult::*;
use std::str;

use std::io::prelude::*;
use std::io::BufReader;
use std::net::TcpStream;
use regex::Regex;

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
        prefix: pr.map(|p| p.to_string()),
        command: c.to_string(),
        parameters: pa.into_iter().map(|p| p.to_string()).collect(),
    } } ));
    
struct MessageIter {
    iter: Box<Iterator<Item=Message>>
}

impl Iterator for MessageIter {
    type Item = Message;

    fn next(&mut self) -> Option<Message> {
        return self.iter.next();
    }
}

struct IRCConnection {
    stream: TcpStream,
}

impl IRCConnection {
    fn connect() -> Self {
        let mut stream = TcpStream::connect("chat.freenode.net:6667").unwrap();
        
        IRCConnection {
            stream: stream,
        }
    }
    
    fn messages(&self) -> MessageIter {
        let reader = BufReader::new(self.stream.try_clone().expect("Could not clone"));
        
        let iter = reader.lines().map(|result| {
            let line = result.expect("No Content");
            let result = deserialize(line.as_bytes());
            match result {
                Done(rest, res) => res,
                Error(err) => panic!("{}", err),
                Incomplete(someting) => panic!("Incomplete: {:?}", someting),
            }
        });
        
        MessageIter{ iter: Box::new(iter) }
    }
    
    fn send(&self, msg: &Message) {
        print_message(msg);
        write!(&self.stream, "{}\r\n", msg.serialize()).expect("Could not write!");
    }
}

fn main() {
    let mut conn = IRCConnection::connect();
    
    conn.send(&Message::new("NICK", &["sindreij_"]));
    conn.send(&Message::new("USER", &["sindreij_", "0", "*", "sindreij_"]));
    
    for msg in conn.messages() {
        incoming(msg, &conn);
    }
}

fn print_message(msg: &Message) {
    println!("{}: {}", msg.command, msg.parameters.join(" "));
}

fn incoming(msg: Message, conn: &IRCConnection) {
    print_message(&msg);
    match msg.command.as_str() {
        "PING" => {
            println!("Got pinged");
            conn.send(&Message{ 
                prefix: None, 
                command: "PONG".to_string(), 
                parameters: msg.parameters
            });
        },
        _ => {}
    }
}