use crate::error::{ErrorKind, RBError};
use crate::s3::{S3Path, RBS3};

use std::ffi::OsStr;
use std::fs::read_dir;
use std::io;
use std::path::Path;

use path_clean::PathClean; // We use canonicalize() for local paths, but path_clean for remote paths

pub async fn list_remote_path(s3: &RBS3, s3_path: S3Path) -> Result<String, RBError> {
    if let S3Path {
        bucket: Some(bucket),
        key,
    } = s3_path
    {
        let key_prefix = key.map(|k| k + "/");
        let files = s3.list_files(bucket, key_prefix).await?;
        if files.is_empty() {
            Ok(String::from("There are no files at this path.\n"))
        } else {
            Ok(files.join("\n"))
        }
    } else {
        let buckets = s3.list_buckets().await?;
        Ok(buckets.join("\n"))
    }
}

pub fn list_local_path(local_path: &Path) -> Result<String, RBError> {
    read_dir(local_path)
        .and_then(|mut entries| {
            let mut dirs: Vec<String> = Vec::new();
            entries.try_for_each(|entry_res| -> Result<(), io::Error> {
                dirs.push(entry_res?.file_name().to_string_lossy().into_owned());
                Ok(())
            })?;
            dirs.sort_unstable();
            Ok(dirs.join("\n"))
        })
        .map_err(RBError::wrap_io)
}

pub async fn get_file(
    s3: &RBS3,
    remote_cwd: &Path,
    local_cwd: &Path,
    remote_source: &String,
    local_destination: &Option<String>,
) -> Result<String, RBError> {
    let source_path = remote_cwd.join(remote_source).clean();
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
        let non_canonical_path = local_cwd.join(local_dest);
        if non_canonical_path.is_dir() {
            // Awesome, this is the happy path!
            let dest_dir = non_canonical_path
                .canonicalize()
                .map_err(RBError::wrap_io)?;

            dest_dir.join(Path::new(
                source_path
                    .file_name()
                    .unwrap_or(OsStr::new("unknown_s3_file")),
            ))
        } else if non_canonical_path.is_file() {
            return Err(RBError::new(ErrorKind::TargetAlreadyExists));
        } else if non_canonical_path
            .to_str()
            .map_or(false, |s| s.ends_with('/') || s.ends_with('\\'))
        {
            // This means the path does not exist, but it ends in a slash, which means that the user
            // expected it to be a directory
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
                let dest_dir = path_without_filename
                    .canonicalize()
                    .map_err(RBError::wrap_io)?;

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

        let dest_filepath = local_cwd.join(dest_filename);
        if dest_filepath.is_file() {
            return Err(RBError::new(ErrorKind::TargetAlreadyExists));
        }
        dest_filepath
    };

    // Okay, after all that, now we have finalized bucket, key, dest_path. Time to download!
    println!(
        "Downloading file '{}'...",
        dest_path
            .file_name()
            .unwrap_or(OsStr::new("unknown"))
            .to_string_lossy()
    );
    s3.download_object(bucket, key, &dest_path).await?;
    Ok(format!(
        "File downloaded successfully: {}",
        dest_path.display()
    ))
}

pub async fn put_file(
    s3: &RBS3,
    remote_cwd: &Path,
    local_cwd: &Path,
    local_source: &String,
    remote_destination: &Option<String>,
) -> Result<String, RBError> {
    let src_path = local_cwd
        .join(local_source)
        .canonicalize()
        .map_err(RBError::wrap_io)?;

    if !src_path.is_file() {
        return Err(RBError::new(ErrorKind::InvalidTarget));
    }

    let dest_path = if let Some(remote_dir) = remote_destination {
        remote_cwd.join(remote_dir).clean()
    } else {
        remote_cwd
            .join(Path::new(
                // Because of the is_file validation on src_path above, we know this path is guaranteed to have a file
                // name
                src_path.file_name().unwrap(),
            ))
            .clean()
    };
    let s3_path = S3Path::try_from_path(&dest_path)?;
    if !s3_path.has_key_and_bucket() {
        return Err(RBError::new(ErrorKind::InvalidTarget));
    }

    let bucket = s3_path.bucket.unwrap();
    let key = s3_path.key.unwrap();

    if s3.object_exists(bucket.clone(), key.clone()).await? {
        return Err(RBError::new(ErrorKind::TargetAlreadyExists));
    }

    // Okay, after all that, now we have finalized bucket, key, src_path. Time to upload!
    println!(
        "Uploading file '{}'...",
        src_path.file_name().unwrap().to_string_lossy()
    );
    s3.put_object(bucket, key, &src_path).await?;
    Ok(format!(
        "File uploaded successfully: {}",
        dest_path.display()
    ))
}
