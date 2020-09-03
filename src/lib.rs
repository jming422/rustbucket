pub mod error;

use crate::error::{ErrorKind, RBError};

use std::io::{stdin, stdout, Write};
use std::iter::Peekable;
use std::str::SplitWhitespace;

#[derive(Debug)]
pub struct Config<'a> {
    pub debug: bool,
    pub single_command: Option<&'a str>,
}

#[derive(Debug, Clone)]
enum Command {
    ListDirectory,
    ChangeDirectory(String),
    GetFile {
        remote_source: String,
        local_destination: String,
    },
    PutFile {
        local_source: String,
        remote_destination: String,
    },
}

fn warn_if_more_words(mut words: Peekable<SplitWhitespace>) {
    if words.peek().is_some() {
        let extra_words = words.count();
        println!(
            "Command doesn't take any more arguments, but {} more were given; ignoring them.",
            extra_words
        );
    }
}

// todo: non-cd commands don't support paths with spaces; none of the commands support quoted or escaped arguments to
// deal with the spaces problem
fn parse_command(cmd_str: &str) -> Result<Command, RBError> {
    let trimmed_lower = cmd_str.trim().to_lowercase();
    let mut words = trimmed_lower.split_whitespace().peekable();

    match words.next().unwrap_or("invalid") {
        "exit" | "quit" => {
            warn_if_more_words(words);
            Err(RBError::new(ErrorKind::UserExit))
        }
        "ls" | "dir" => {
            warn_if_more_words(words);
            Ok(Command::ListDirectory)
        }
        "cd" => match words.next() {
            Some(next_word) => {
                let rest_words = words.fold(next_word.to_owned(), |acc, word| acc + " " + word);
                Ok(Command::ChangeDirectory(rest_words))
            }
            None => Err(RBError::new(ErrorKind::InvalidTarget)),
        },
        "get" => {
            let source = words.next().ok_or(RBError::new(ErrorKind::InvalidTarget))?;
            let destination = words.next().ok_or(RBError::new(ErrorKind::InvalidTarget))?;
            warn_if_more_words(words);
            Ok(Command::GetFile {
                remote_source: source.to_owned(),
                local_destination: destination.to_owned(),
            })
        }
        "put" => {
            let source = words.next().ok_or(RBError::new(ErrorKind::InvalidTarget))?;
            let destination = words.next().ok_or(RBError::new(ErrorKind::InvalidTarget))?;
            warn_if_more_words(words);
            Ok(Command::PutFile {
                local_source: source.to_owned(),
                remote_destination: destination.to_owned(),
            })
        }
        // todo: mget? mput?
        _ => Err(RBError::new(ErrorKind::InvalidCommand)),
    }
}

fn run_command<'a, 'b>(cmd: &'a Command) -> Result<Option<&'b str>, RBError> {
    // todo impl
    Ok(None)
}

fn read_input() -> Result<String, RBError> {
    let mut input = String::new();

    match stdin().read_line(&mut input) {
        Ok(_) => Ok(input),
        Err(e) => Err(RBError::new_with_source(ErrorKind::IO, e)),
    }
}

static INVALID_COMMAND_WARNING: &str = "Unknown command. For available commands,";
static INVALID_TARGET_WARNING: &str = "Invalid argument(s) for this command";

pub fn run(config: Config) -> Result<(), RBError> {
    if let Some(cmd_input) = config.single_command {
        return match parse_command(cmd_input) {
            Err(e) => match e.kind() {
                ErrorKind::UserExit => Ok(()),
                ErrorKind::InvalidCommand => {
                    eprintln!("{} run with --help", INVALID_COMMAND_WARNING);
                    Err(e)
                }
                ErrorKind::InvalidTarget => {
                    eprintln!("{}", INVALID_TARGET_WARNING);
                    Err(e)
                }
                _ => Err(e),
            },
            Ok(cmd) => {
                println!("{}", run_command(&cmd)?.unwrap_or("ok"));
                Ok(())
            }
        };
    }

    loop {
        print!("> ");
        stdout()
            .flush()
            .or_else(|io_err| Err(RBError::new_with_source(ErrorKind::IO, io_err)))?;

        let input = read_input()?;
        let cmd_res = parse_command(&input);
        if let Err(e) = cmd_res {
            match e.kind() {
                ErrorKind::UserExit => break,
                ErrorKind::InvalidCommand => {
                    println!("{} type \"help\"", INVALID_COMMAND_WARNING);
                    continue;
                }
                ErrorKind::InvalidTarget => {
                    println!("{}", INVALID_TARGET_WARNING);
                    continue;
                }
                _ => return Err(e),
            };
        }

        let cmd = cmd_res.unwrap();
        match run_command(&cmd) {
            Ok(s) => println!("{}", s.unwrap_or("ok")),
            Err(e) => match e.kind() {
                ErrorKind::InvalidTarget => println!("{}", INVALID_TARGET_WARNING),
                _ => return Err(e),
            },
        };
    }

    Ok(())
}
