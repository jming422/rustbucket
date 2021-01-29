pub mod error;
mod s3;

use crate::error::{ErrorKind, RBError};
use crate::s3::{S3Path, RBS3};

use std::env::{current_dir, set_current_dir};
use std::ffi::OsStr;
use std::fs::read_dir;
use std::io;
use std::iter::Peekable;
use std::path::{Path, PathBuf};
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
    let trimmed_lower = trimmed.to_lowercase();
    let mut words = trimmed_lower.split_whitespace().peekable();

    match words.next().unwrap_or("invalid") {
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
                Ok(s3_path) => {
                    if let S3Path {
                        bucket: Some(bucket),
                        key,
                    } = s3_path
                    {
                        let files = self.s3.list_files(bucket, key).await?;
                        if files.is_empty() {
                            Ok(String::from("There are no files at this path.\n"))
                        } else {
                            Ok(files.join("\n"))
                        }
                    } else {
                        let buckets = self.s3.list_buckets().await?;
                        Ok(buckets.join("\n"))
                    }
                }
                Err(e) if e.kind() == ErrorKind::InvalidTarget => {
                    println!("No valid S3 bucket path provided! Resetting remote path to '/' and listing all available buckets");
                    self.remote_cwd = PathBuf::from("/");
                    let buckets = self.s3.list_buckets().await?;
                    Ok(buckets.join("\n"))
                }
                Err(e) => Err(e),
            },
            Command::ListLocalDirectory => read_dir(&self.local_cwd)
                .and_then(|mut entries| {
                    let mut dirs: Vec<String> = Vec::new();
                    entries.try_for_each(|entry_res| -> Result<(), io::Error> {
                        dirs.push(entry_res?.file_name().to_string_lossy().into_owned());
                        Ok(())
                    })?;
                    dirs.sort_unstable();
                    Ok(dirs.join("\n"))
                })
                .map_err(|e| RBError::new_with_source(ErrorKind::IO, e)),
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
                        _ => Err(RBError::new_with_source(ErrorKind::IO, io_err)),
                    },
                }
            }
            Command::GetFile {
                remote_source,
                local_destination,
            } => {
                let source_path = self.remote_cwd.join(remote_source).clean();
                let s3_path = S3Path::try_from_path(&source_path)?;
                if !s3_path.has_key_and_bucket() {
                    return Err(RBError::new(ErrorKind::InvalidTarget));
                }
                let bucket = s3_path.bucket.unwrap();
                let key = s3_path.key.unwrap();

                let dest_path = if let Some(local_dest) = local_destination {
                    // We want to canonicalize this path so that we ensure that whatever directory local_destination
                    // puts us in actually exists. It's valid for local_destination to either include or omit a
                    // terminating filename, so we have to deal with that too.
                    let non_canonical_path = self.local_cwd.join(local_dest);
                    if non_canonical_path.is_dir() {
                        // Awesome, this is the happy path!
                        let dest_dir = non_canonical_path
                            .canonicalize()
                            .map_err(|io_err| RBError::new_with_source(ErrorKind::IO, io_err))?;

                        dest_dir.join(Path::new(
                            source_path
                                .file_name()
                                .unwrap_or(OsStr::new("unknown_s3_file")),
                        ))
                    } else if non_canonical_path.is_file()
                        || non_canonical_path
                            .to_str()
                            .map_or(false, |s| s.ends_with('/') || s.ends_with('\\'))
                    {
                        // This means the path either:
                        //   - points to a regular file that already exists, or
                        //   - does not exist, but it ends in a slash, which means that the user expected it to be a
                        //     directory
                        return Err(RBError::new(ErrorKind::InvalidTarget));
                    } else {
                        // This means that the path does not exist on disk, and the user didn't end the path with a
                        // slash, so the last path component is their intended destination filename. We have to do one
                        // last check that the path without their destination filename ending is a directory, and we can
                        // do this by pop()-ing off their filename and checking is_dir():
                        let mut path_without_filename = non_canonical_path.clone();
                        path_without_filename.pop();
                        if path_without_filename.is_dir() {
                            // OK!
                            let dest_dir =
                                path_without_filename.canonicalize().map_err(|io_err| {
                                    RBError::new_with_source(ErrorKind::IO, io_err)
                                })?;

                            dest_dir.join(Path::new(
                                non_canonical_path
                                    .file_name()
                                    .or(source_path.file_name())
                                    .unwrap_or(OsStr::new("unknown_s3_file")),
                            ))
                        } else {
                            // Destination directory doesn't exist, error
                            return Err(RBError::new(ErrorKind::InvalidTarget));
                        }
                    }
                } else {
                    // No destination path was provided; just use local_cwd.
                    let dest_filename = source_path
                        .file_name()
                        .ok_or(RBError::new(ErrorKind::Other))?; // This should never happen thanks to set_current_dir() earlier

                    self.local_cwd.join(dest_filename)
                };

                // Okay, after all that, now we have finalized bucket, key, dest_path. Time to download!
                println!(
                    "Downloading file '{}'...",
                    dest_path
                        .file_name()
                        .unwrap_or(OsStr::new("unknown"))
                        .to_string_lossy()
                );
                self.s3.download_object(bucket, key, &dest_path).await?;
                Ok(format!(
                    "File downloaded successfully: {}",
                    dest_path.display()
                ))
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

async fn run_loop(rl: &mut rustyline::Editor<()>, mut runner: Runner) -> Result<(), RBError> {
    loop {
        match rl.readline("> ") {
            Err(ReadlineError::Interrupted) => break,
            Err(ReadlineError::Eof) => break,
            Err(e) => return Err(RBError::new_with_source(ErrorKind::IO, e)),
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
                        _ => return Err(e),
                    };
                }

                let cmd = cmd_res.unwrap();
                match runner.run_command(&cmd).await {
                    Ok(s) => println!("{}", s),
                    Err(e) => match e.kind() {
                        // TODO: Add better UX for "gracefully" handling S3 and IO error types
                        ErrorKind::InvalidTarget => println!("{}", INVALID_TARGET_WARNING),
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
