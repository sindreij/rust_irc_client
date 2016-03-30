#[macro_use]
extern crate nom;
extern crate rand;

use std::str;
use std::fmt;
use std::net::ToSocketAddrs;
use std::io::prelude::*;
use std::io::BufReader;
use std::net::TcpStream;
use rand::distributions::{IndependentSample, Range};

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

    fn nickname<'a>(&'a self) -> Option<&'a str> {
        match &self.prefix {
            &Some(ref prefix) => prefix.split('!').next(),
            &None => None
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
    fn connect<A: ToSocketAddrs>(addr: A) -> Self {
        let stream = TcpStream::connect(addr).unwrap();

        IRCConnection {
            stream: stream,
            nick: "".to_owned(),
        }
    }

    fn set_nick(&mut self, nick: &str) {
        self.nick = nick.to_owned();
        self.send(&Message::new("NICK", &[nick]));
    }

    fn set_user(&self, username: &str, realname: &str) {
        self.send(&Message::new("USER", &[username, "0", "*", realname]));
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

fn message_from_command(reply_chan: &str, parsed: ParsedText) -> Option<Message> {
    match parsed.command {
        "sum" => {
            let sum = parsed.parameters.iter().filter_map(|n| n.parse::<i64>().ok())
                      .fold(0, |acc, x| acc + x);
            Some(Message::new("PRIVMSG", &[reply_chan.to_owned(), format!("The sum is {}", sum)]))
        },
        "echo" => {
            let echo = parsed.parameters.join(" ");
            Some(Message::new("PRIVMSG", &[reply_chan.to_owned(), format!("{}", echo)]))
        },
        "random" => {
            if parsed.parameters.len() != 1 {
                return Some(Message::new("PRIVMSG", &[reply_chan.to_owned(), format!("Random take one parameter")]));
            }
            let mut rng = rand::thread_rng();
            let max:i64 = match parsed.parameters[0].parse() {
                Ok(num) => num,
                Err(_) => return Some(Message::new("PRIVMSG", &[reply_chan.to_owned(), format!("{} is not a number", parsed.parameters[0])])),
            };
            if max < 0 {
                return Some(Message::new("PRIVMSG", &[reply_chan.to_owned(), format!("{} is below 0", max)]));
            }
            let random:i64 = Range::new(0, max+1).ind_sample(&mut rng);

            Some(Message::new("PRIVMSG", &[reply_chan.to_owned(), format!("{}", random)]))
        },
        _ => None,
    }
}

struct IRCApp {
    conn: IRCConnection,
    channels: String,
    join_message: String,
}

impl IRCApp {
    fn react_to_text(&self, from: Option<&str>, chan: &str, text: &str) {
        let parsed = deserialize_text(text.as_bytes());
        let msg = if let IResult::Done(_, parsed) = parsed {
            match (parsed.mention, from) {
                (_, Some(from)) if chan == self.conn.nick => {
                    message_from_command(from, parsed)
                }
                (Some(mention), _) if mention == self.conn.nick => {
                    message_from_command(chan, parsed)
                },
                _ => None
            }
        } else {
            None
        };

        if let Some(msg) = msg {
            self.conn.send(&msg);
        };
    }

    fn incoming(&self, msg: Message) {
        println!("< {}", msg);
        match msg.command.as_str() {
            "PING" => {
                let params:Vec<_> = msg.parameters.iter().map(|s| s.as_str()).collect();
                self.conn.send(&Message::new("PONG", &params));
            },
            "376" => {
                self.conn.send(&Message::new("JOIN", &[self.channels.as_str()]));
            },
            "PRIVMSG" => {
                let channel = &msg.parameters[0];
                let text = &msg.parameters[1];
                self.react_to_text(msg.nickname(), channel, text);
            },
            "JOIN" => {
                if let Some(nick) = msg.nickname() {
                    if nick == self.conn.nick {
                        for chan in (&msg.parameters[0]).split(',') {
                            self.conn.send(&Message::new("PRIVMSG", &[chan, &self.join_message]));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn run(&self) {
        for msg in self.conn.messages() {
            self.incoming(msg);
        }
    }
}

fn main() {
    let input = "chat.freenode.net:6667\nrhenium\nrhenium\nrhenium\n#botters-test,#rhenium-bottest\nHello World!!!";
    let parts:Vec<_> = input.lines().collect();
    let mut conn = IRCConnection::connect(parts[0]);
    conn.set_nick(parts[1]);
    conn.set_user(parts[2], parts[3]);

    let app = IRCApp {
        conn: conn,
        channels: parts[4].to_owned(),
        join_message: parts[5].to_owned(),
    };

    app.run();
}
