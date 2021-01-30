use crate::error::{ErrorKind, RBError};

use std::default::Default;
use std::path::{Component, Path};

use rusoto_core::ByteStream;
use rusoto_s3::{GetObjectRequest, ListObjectsV2Request, PutObjectRequest, S3Client, S3};
use tokio::{fs::File, io};
use tokio_util::io::ReaderStream;

pub struct S3Path {
    pub bucket: Option<String>,
    pub key: Option<String>,
}

impl S3Path {
    pub fn has_bucket(&self) -> bool {
        self.bucket.is_some()
    }

    pub fn has_key_and_bucket(&self) -> bool {
        self.has_bucket() && self.key.is_some()
    }

    pub fn try_from_path(path: &Path) -> Result<Self, RBError> {
        let mut components = path.components();
        let maybe_bucket = components
            .find_map(|c| match c {
                // This will find the first non-prefix path element, skipping weird Windows prefixes
                Component::Normal(bucket) => Some(Ok(bucket)),
                Component::ParentDir => Some(Err(RBError::new(ErrorKind::InvalidTarget))),
                Component::CurDir => Some(Err(RBError::new(ErrorKind::InvalidTarget))),
                _ => None,
            })
            .transpose()?;

        match maybe_bucket {
            None => Ok(Self {
                bucket: None,
                key: None,
            }),
            Some(bucket_component) => {
                let remaining_path = components.as_path();

                // to_str only returns None if the path is not valid unicode. Since we created and modified
                // these paths exclusively using the &str/String types, they are guaranteed to always be valid
                // unicode, so we can unwrap() them safely.
                let bucket = Some(bucket_component.to_str().unwrap().to_owned());
                let key_str = remaining_path.to_str().unwrap();
                let key = if key_str.is_empty() {
                    None
                } else {
                    Some(key_str.to_owned())
                };

                println!(
                    "Debug: generated S3Path with bucket {:?} and key {:?}",
                    bucket, key
                );
                Ok(Self { bucket, key })
            }
        }
    }
}

pub struct RBS3 {
    client: S3Client,
}

impl RBS3 {
    pub fn new() -> Self {
        RBS3 {
            client: S3Client::new(Default::default()),
        }
    }

    pub async fn list_buckets(&self) -> Result<Vec<String>, RBError> {
        let result = self.client.list_buckets().await.map_err(RBError::wrap_s3)?;

        let buckets: Vec<String> = result
            .buckets
            .unwrap_or(vec![])
            .into_iter()
            .filter_map(|bucket| bucket.name)
            .collect();

        Ok(buckets)
    }

    pub async fn list_files(
        &self,
        bucket: String,
        prefix: Option<String>,
    ) -> Result<Vec<String>, RBError> {
        println!(
            "Debug: listing files at bucket {}, prefix {}",
            bucket,
            prefix.as_ref().unwrap_or(&String::from("<no prefix>"))
        );
        let mut params = ListObjectsV2Request {
            bucket,
            prefix: prefix.clone(),
            delimiter: Some(String::from("/")),
            ..Default::default()
        };

        let mut results: Vec<String> = Vec::new();
        let mut files: Vec<String> = Vec::new();

        loop {
            let output = self
                .client
                .list_objects_v2(params.clone())
                .await
                .map_err(RBError::wrap_s3)?;

            if let Some(prefixes) = output.common_prefixes {
                results.extend(
                    prefixes
                        .into_iter()
                        .filter_map(|object| object.prefix)
                        .filter_map(|common_prefix| {
                            if let Some(pfx_str) = prefix.as_ref().map(|pfx| pfx.as_str()) {
                                common_prefix
                                    .strip_prefix(pfx_str)
                                    .map(|cleaned_str| cleaned_str.to_owned())
                            } else {
                                Some(common_prefix)
                            }
                        }),
                );
            }

            if let Some(objects) = output.contents {
                files.extend(
                    objects
                        .into_iter()
                        .filter_map(|object| object.key)
                        .filter_map(|key| {
                            if let Some(pfx_str) = prefix.as_ref().map(|pfx| pfx.as_str()) {
                                key.strip_prefix(pfx_str).and_then(|key_no_prefix| {
                                    if !key_no_prefix.contains('/') {
                                        Some(key_no_prefix.to_owned())
                                    } else {
                                        None
                                    }
                                })
                            } else if !key.contains('/') {
                                Some(key)
                            } else {
                                None
                            }
                        }),
                );
            };

            // It's convenient to not use `if let Some()` here because params.continuation_token is also an Option
            if output.next_continuation_token.is_some() {
                params.continuation_token = output.next_continuation_token;
            // don't break; we'll loop and call the s3 function again with the new continuation token
            } else {
                break;
            }
        }

        // Do this stuff at the end so that all the directories appear at the top and the files at the bottom
        results.sort_unstable();
        files.sort_unstable();
        results.extend(files);

        Ok(results)
    }

    pub async fn object_exists(&self, bucket: String, key: String) -> Result<bool, RBError> {
        println!(
            "Debug: Checking if file exists at bucket {}, key {}",
            bucket, key
        );
        let params = ListObjectsV2Request {
            bucket,
            prefix: Some(key),
            ..Default::default()
        };

        let output = self
            .client
            .list_objects_v2(params.clone())
            .await
            .map_err(RBError::wrap_s3)?;

        Ok(output.key_count.map_or(false, |count| count != 0))
    }

    pub async fn download_object(
        &self,
        bucket: String,
        key: String,
        dest_path: &Path,
    ) -> Result<(), RBError> {
        println!(
            "Debug: downloading bucket {} key {} to file {:?}",
            bucket, key, dest_path
        );
        let params = GetObjectRequest {
            bucket,
            key: key.clone(),
            ..Default::default()
        };

        let mut dest_file = File::create(dest_path).await.map_err(RBError::wrap_io)?;

        let object = self
            .client
            .get_object(params)
            .await
            .map_err(RBError::wrap_s3)?;

        if let Some(body) = object.body {
            let mut object_stream = body.into_async_read();
            io::copy(&mut object_stream, &mut dest_file)
                .await
                .map_err(RBError::wrap_io)?;

            Ok(())
        } else {
            eprintln!("Object at key {} has no body!", key);
            Err(RBError::new(ErrorKind::S3))
        }
    }

    pub async fn put_object(
        &self,
        bucket: String,
        key: String,
        source_path: &Path,
    ) -> Result<(), RBError> {
        println!(
            "Debug: uploading file {:?} to bucket {} key {}",
            source_path, bucket, key
        );

        let src_file = File::open(source_path).await.map_err(RBError::wrap_io)?;

        let params = PutObjectRequest {
            bucket,
            key,
            body: Some(ByteStream::new(ReaderStream::new(src_file))),
            ..Default::default()
        };

        self.client
            .put_object(params)
            .await
            .map_err(RBError::wrap_s3)?;

        Ok(())
    }
}
