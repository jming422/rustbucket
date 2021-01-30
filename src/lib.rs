mod commands;
pub mod error;
mod s3;

use crate::error::{ErrorKind, RBError};
use crate::s3::{S3Path, RBS3};

use std::env::{current_dir, set_current_dir};
use std::io;
use std::iter::Peekable;
use std::path::PathBuf;
use std::str::SplitWhitespace;

use path_clean::PathClean; // We use canonicalize() for local paths, but path_clean for remote paths
use rustyline::error::ReadlineError;

#[derive(Debug)]
pub struct Config {
    pub debug: bool,
    pub single_command: Option<String>,
}

#[derive(Debug, Clone)]
enum Command {
    ListRemoteDirectory,
    ListLocalDirectory,
    PrintRemoteDirectory,
    PrintLocalDirectory,
    ChangeRemoteDirectory(String),
    ChangeLocalDirectory(String),
    GetFile {
        remote_source: String,
        local_destination: Option<String>,
    },
    PutFile {
        local_source: String,
        remote_destination: Option<String>,
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
fn parse_command(cmd_str: String) -> Result<Command, RBError> {
    let trimmed = cmd_str.trim();
    let mut words = trimmed.split_whitespace().peekable();

    match words.next().unwrap_or("invalid").to_lowercase().as_str() {
        "exit" | "quit" => {
            warn_if_more_words(words);
            Err(RBError::new(ErrorKind::UserExit))
        }
        "ls" | "dir" => {
            warn_if_more_words(words);
            Ok(Command::ListRemoteDirectory)
        }
        "lls" | "ldir" => {
            warn_if_more_words(words);
            Ok(Command::ListLocalDirectory)
        }
        "pwd" => {
            warn_if_more_words(words);
            Ok(Command::PrintRemoteDirectory)
        }
        "lpwd" => {
            warn_if_more_words(words);
            Ok(Command::PrintLocalDirectory)
        }
        "cd" => match words.next() {
            Some(_) => {
                let cmd_arg = trimmed
                    .strip_prefix("cd ") // Below error only possible if non-space whitespace was used
                    .ok_or_else(|| RBError::new(ErrorKind::InvalidTarget))?
                    .trim();
                Ok(Command::ChangeRemoteDirectory(cmd_arg.to_owned()))
            }
            None => Err(RBError::new(ErrorKind::InvalidTarget)),
        },
        "lcd" => match words.next() {
            Some(_) => {
                let cmd_arg = trimmed
                    .strip_prefix("lcd ") // Below error only possible if non-space whitespace was used
                    .ok_or_else(|| RBError::new(ErrorKind::InvalidTarget))?
                    .trim();
                Ok(Command::ChangeLocalDirectory(cmd_arg.to_owned()))
            }
            None => Err(RBError::new(ErrorKind::InvalidTarget)),
        },
        "get" => {
            let source = words.next().ok_or(RBError::new(ErrorKind::InvalidTarget))?;
            let destination = words.next();
            warn_if_more_words(words);
            Ok(Command::GetFile {
                remote_source: source.to_owned(),
                local_destination: destination.map(|dest_str| dest_str.to_owned()),
            })
        }
        "put" => {
            let source = words.next().ok_or(RBError::new(ErrorKind::InvalidTarget))?;
            let destination = words.next();
            warn_if_more_words(words);
            Ok(Command::PutFile {
                local_source: source.to_owned(),
                remote_destination: destination.map(|dest_str| dest_str.to_owned()),
            })
        }
        // todo: mget? mput?
        _ => Err(RBError::new(ErrorKind::InvalidCommand)),
    }
}

struct Runner {
    local_cwd: PathBuf,
    remote_cwd: PathBuf,
    s3: RBS3,
}

impl Runner {
    fn new(local_cwd: PathBuf, remote_cwd: PathBuf) -> Self {
        Runner {
            local_cwd,
            remote_cwd,
            s3: RBS3::new(),
        }
    }

    async fn run_command(&mut self, cmd: &Command) -> Result<String, RBError> {
        match cmd {
            Command::PrintRemoteDirectory => Ok(format!(
                "Remote directory is now: {}",
                self.remote_cwd.display()
            )),
            Command::PrintLocalDirectory => Ok(format!(
                "Local directory is now: {}",
                self.local_cwd.display()
            )),
            Command::ListRemoteDirectory => match S3Path::try_from_path(&self.remote_cwd) {
                Ok(s3_path) => commands::list_remote_path(&self.s3, s3_path).await,
                Err(e) if e.kind() == ErrorKind::InvalidTarget => {
                    println!("No valid S3 bucket path provided! Resetting remote path to '/' and listing all available buckets");
                    self.remote_cwd = PathBuf::from("/");
                    let buckets = self.s3.list_buckets().await?;
                    Ok(buckets.join("\n"))
                }
                Err(e) => Err(e),
            },
            Command::ListLocalDirectory => commands::list_local_path(&self.local_cwd),
            Command::ChangeRemoteDirectory(dir) => {
                // TODO: use S3 to validate that the requested bucket and prefix path exist
                self.remote_cwd.push(dir);
                self.remote_cwd = self.remote_cwd.clean();
                Ok(format!(
                    "Remote directory is now: {}",
                    self.remote_cwd.display()
                ))
            }
            Command::ChangeLocalDirectory(dir) => {
                let new_path = self.local_cwd.join(dir);
                let canonical_path = new_path.canonicalize().and_then(|canonical_path| {
                    set_current_dir(&canonical_path)?;
                    Ok(canonical_path)
                });
                match canonical_path {
                    Ok(good_new_path) => {
                        self.local_cwd = good_new_path;
                        Ok(format!(
                            "Local directory is now: {}",
                            self.local_cwd.display()
                        ))
                    }
                    Err(io_err) => match io_err.kind() {
                        io::ErrorKind::NotFound => {
                            Ok(format!("Directory not found: {}", new_path.display()))
                        }
                        io::ErrorKind::InvalidInput => {
                            Ok(format!("Invalid path: {}", new_path.display()))
                        }
                        _ => Err(RBError::wrap_io(io_err)),
                    },
                }
            }
            Command::GetFile {
                remote_source,
                local_destination,
            } => {
                commands::get_file(
                    &self.s3,
                    &self.remote_cwd,
                    &self.local_cwd,
                    remote_source,
                    local_destination,
                )
                .await
            }
            Command::PutFile {
                local_source,
                remote_destination,
            } => {
                // todo impl
                Ok(String::from("ok"))
            }
        }
    }
}

static INVALID_COMMAND_WARNING: &str = "Unknown command. For available commands,";
static INVALID_TARGET_WARNING: &str = "Invalid argument(s) for this command";
static TARGET_EXISTS_WARNING: &str = "The specified file already exists, doing nothing";

async fn run_loop(rl: &mut rustyline::Editor<()>, mut runner: Runner) -> Result<(), RBError> {
    loop {
        match rl.readline("> ") {
            Err(ReadlineError::Interrupted) => break,
            Err(ReadlineError::Eof) => break,
            Err(e) => return Err(RBError::wrap_io(e)),
            Ok(line) => {
                let cmd_res = parse_command(line);
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
                        ErrorKind::TargetAlreadyExists => {
                            println!("{}", TARGET_EXISTS_WARNING);
                            continue;
                        }
                        _ => return Err(e),
                    };
                }

                let cmd = cmd_res.unwrap();
                match runner.run_command(&cmd).await {
                    Ok(s) => println!("{}", s),
                    Err(e) => match e.kind() {
                        // TODO: Add better UX for "gracefully" handling S3 and IO error types
                        ErrorKind::InvalidTarget => println!("{}", INVALID_TARGET_WARNING),
                        ErrorKind::TargetAlreadyExists => println!("{}", TARGET_EXISTS_WARNING),
                        _ => return Err(e),
                    },
                };
            }
        };
    }
    Ok(())
}

pub async fn run(config: Config) -> Result<(), RBError> {
    let mut runner = Runner::new(
        current_dir().unwrap_or(PathBuf::from("~")),
        PathBuf::from("/"),
    );

    // Single command passed with flag
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
                ErrorKind::TargetAlreadyExists => {
                    eprintln!("{}", TARGET_EXISTS_WARNING);
                    Err(e)
                }
                _ => Err(e),
            },
            Ok(cmd) => {
                // It's cool if this one has no error handling besides, "exit with the error," since it's running as a
                // one-off command anyway
                println!("{}", runner.run_command(&cmd).await?);
                Ok(())
            }
        };
    }

    // Interactive prompt mode
    let mut rl = rustyline::Editor::<()>::new();
    // if let Err(e) = rl.load_history(&history_path) {
    //     println!("No previous history. (error: {})", e);
    // }
    let result = run_loop(&mut rl, runner).await;
    // if let Err(e) = rl.save_history(&history_path) {
    //     eprintln!("Error trying to save interactive prompt history: {}", e);
    // }

    result
}
