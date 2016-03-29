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
    fn new<P: Into<String> + Clone>(command: &str, parameters: &[P]) -> Self {
        let params = parameters.to_vec();
        Message {
            prefix: None,
            command: command.to_owned(),
            parameters: params.into_iter().map(|s| s.into()).collect(),
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
        let prefix = match &self.prefix {
            &Some(ref pref) => format!("({}) ", pref),
            &None => "".to_owned(),
        };
        write!(f, "{}{}: {}", prefix, self.command, self.parameters.join(" "))
    }
}

named!(prefix<&str>, map_res!(delimited!(char!(':'), is_not!(" "), char!(' ')), str::from_utf8));
named!(command<&str>, map_res!(is_not!(" "), str::from_utf8));
named!(parameter, alt!(preceded!(char!(':'), nom::rest) | is_not!(" ")));
named!(parameters< Vec<&str> >, separated_list!(char!(' '), map_res!(parameter, str::from_utf8)));

named!(mention<&str>, map_res!(terminated!(is_not!(": "), tag!(": ")), str::from_utf8));

named!(deserialize_message<Message>, chain!(
    pr: prefix? ~
    c: command ~
    char!(' ') ~
    pa: parameters
    , || { Message {
        prefix: pr.map(|p| p.to_owned()),
        command: c.to_owned(),
        parameters: pa.into_iter().map(|p| p.to_owned()).collect(),
        }
    }
));

#[derive(Debug)]
struct ParsedText<'a> {
    mention: Option<&'a str>,
    command: &'a str,
    parameters: Vec<&'a str>,
}

named!(deserialize_text<ParsedText>, chain!(
    m: mention? ~
    c: command ~
    char!(' ') ~
    p: parameters,
    || { ParsedText {
            mention: m,
            command: c,
            parameters: p,
        }
    }
));

struct IRCConnection {
    stream: TcpStream,
    nick: String,
}

impl IRCConnection {
    fn connect<A: ToSocketAddrs>(addr: A, nick: &str) -> Self {
        let stream = TcpStream::connect(addr).unwrap();

        IRCConnection {
            stream: stream,
            nick: nick.to_owned(),
        }
    }

    fn messages(&self) -> Box<Iterator<Item=Message>> {
        let reader = BufReader::new(self.stream.try_clone().expect("Could not clone"));

        let iter = reader.lines().map(|result| {
            let line = result.expect("No Content");
            let result = deserialize_message(line.as_bytes());
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

fn react_to_text(chan: &str, text: &str, conn: &IRCConnection) {
    if text == "Hello World!" {
        conn.send(&Message::new("PRIVMSG", &[chan, "Hello World"]));
    }

    let parsed = deserialize_text(text.as_bytes());

    if let IResult::Done(_, parsed) = parsed {
        if let Some(mention) = parsed.mention {
            if mention == conn.nick {
                match parsed.command {
                    "sum" => {
                        let sum = parsed.parameters.iter().filter_map(|n| n.parse::<i64>().ok())
                                  .fold(0, |acc, x| acc + x);
                        conn.send(&Message::new("PRIVMSG", &[chan.to_owned(), format!("The sum is {}", sum)]));
                    },
                    "echo" => {
                        let echo = parsed.parameters.join(" ");
                        conn.send(&Message::new("PRIVMSG", &[chan.to_owned(), format!("{}", echo)]));
                    },
                    _ => {},
                }
            }
        }
    }
}

fn incoming(msg: Message, conn: &IRCConnection) {
    println!("< {}", msg);
    match msg.command.as_str() {
        "PING" => {
            let params:Vec<_> = msg.parameters.iter().map(|s| s.as_str()).collect();
            conn.send(&Message::new("PONG", &params));
        },
        "376" => {
            conn.send(&Message::new("JOIN", &["#sindreij_bottest"]));
        },
        "PRIVMSG" => {
            let channel = &msg.parameters[0];
            let text = &msg.parameters[1];
            react_to_text(channel, text, conn);
        }
        _ => {}
    }
}

fn main() {
    let input = "chat.freenode.net:6667\nrhenium\nrhenium\nrhenium";
    let parts:Vec<_> = input.lines().collect();
    let conn = IRCConnection::connect(parts[0], parts[1]);

    conn.send(&Message::new("NICK", &[parts[1]]));
    conn.send(&Message::new("USER", &[parts[2], "0", "*", parts[3]]));

    for msg in conn.messages() {
        incoming(msg, &conn);
    }
}