use std::str::from_utf8;
use nom::{space, IResult, Needed};
use nom::IResult::*;

#[macro_use]
named!(nick_parser, chain!(nick: take_until!("!") ~ tag!("!"), ||{nick}));
named!(user_parser, chain!(user: take_until!("@") ~ tag!("@"), ||{user}));
named!(word_parser, take_until!(" "));

#[derive(Debug)]
pub enum Prefix<'a> {
    User(&'a str, &'a str, &'a str),
    Server(&'a str)
}

named!(prefix_parser <&[u8], Prefix>,
    chain!(
        tag!(":")? ~
        hostprefix: host_parser? ~
        serverprefix: map_res!(word_parser, from_utf8)? ~
        space,
        || {
            match hostprefix {
                Some((nick, user, host)) => Prefix::User(nick, user, host),
                None => Prefix::Server(serverprefix.unwrap())
            }
        }
    )
);
named!(host_parser <&[u8], (&str, &str, &str)>,
    chain!(
       nick: map_res!(nick_parser, from_utf8) ~
       user: map_res!(user_parser, from_utf8) ~
       host: map_res!(word_parser, from_utf8) ,
       ||{(nick, user, host)}
    )
);

#[cfg(test)]
mod tests {
    use super::*;
    use nom::IResult::*;
    #[test]
    fn test_parsing_user() {
        match super::prefix_parser(b":user!host@example.com ") {
            Done(left, Prefix::User(nick, user, host)) => {
                assert_eq!(nick, "user");
                assert_eq!(user, "host");
                assert_eq!(host, "example.com");
                assert_eq!(left.len(), 0);
            },
            Incomplete(i) => panic!(format!("Incomplete: {:?}", i)),
            _ => panic!("Error while parsing prefix")
        }
    }
    #[test]
    fn test_parsing_prefix() {
        match super::prefix_parser(b":this.represents.a.server.prefix ") {
            Done(left, Prefix::Server(server)) => {
                assert_eq!(server, "this.represents.a.server.prefix");
                assert_eq!(left.len(), 0);
            },
            Incomplete(i) => panic!(format!("Incomplete: {:?}", i)),
            _ => panic!("Error while parsing prefix")
        }
    }
}
