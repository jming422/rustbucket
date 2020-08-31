pub mod error;

use crate::error::{ErrorKind, RBError};

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
    Exit,
}

fn parse_command(cmd_str: &str) -> Result<Command, RBError> {
    // todo impl
    Ok(Command::ListDirectory)
}

fn run_command(cmd_str: &str) -> Result<Option<&str>, RBError> {
    let cmd = parse_command(cmd_str)?;
    // todo impl
    Ok(None)
}

fn read_input() -> String {
    String::from("todo")
}

pub fn run(config: Config) -> Result<(), RBError> {
    if config.single_command.is_some() {
        println!(
            "{}",
            run_command(config.single_command.unwrap())?.unwrap_or("")
        );
        return Ok(());
    }

    let mut i = 0;
    while i < 3 {
        print!("> ");
        let input = read_input();
        let res = run_command(&input);
        match res {
            Ok(s) => println!("{}", s.unwrap_or("ok")),
            Err(e) => match e.kind() {
                ErrorKind::UserExit => break,
                ErrorKind::InvalidInput => {
                    println!("Unknown command. For available commands type, \"help\"")
                }
                _ => return Err(e),
            },
        }

        i += 1;
    }

    Ok(())
}
