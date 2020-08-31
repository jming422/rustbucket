pub mod error;

use crate::error::{ErrorKind, RBError};

use std::io::{stdin, stdout, Write};

#[derive(Debug)]
pub struct Config<'a> {
    pub debug: bool,
    pub single_command: Option<&'a str>,
}

#[derive(Debug, Copy, Clone)]
enum Command<'a> {
    ListDirectory,
    ChangeDirectory(&'a str),
    GetFile(&'a str),
    PutFile(&'a str),
}

fn parse_command(cmd_str: &str) -> Result<Command, RBError> {
    // todo impl this for real
    let trimmed = cmd_str.trim();
    if trimmed == "exit" {
        Err(RBError::new(ErrorKind::UserExit))
    } else if trimmed == "ls" {
        Ok(Command::ListDirectory)
    } else {
        Err(RBError::new(ErrorKind::InvalidCommand))
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

pub fn run(config: Config) -> Result<(), RBError> {
    if config.single_command.is_some() {
        // println!(
        //     "{}",
        //     run_command(config.single_command.unwrap())?.unwrap_or("")
        // );
        // return Ok(());
        // todo impl
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
                    println!("Unknown command. For available commands type, \"help\"");
                    continue;
                }
                _ => return Err(e),
            };
        }

        let cmd = cmd_res.unwrap();
        match run_command(&cmd) {
            Ok(s) => println!("{}", s.unwrap_or("ok")),
            Err(e) => match e.kind() {
                ErrorKind::InvalidTarget => println!("Invalid target"),
                _ => return Err(e),
            },
        };
    }

    Ok(())
}
