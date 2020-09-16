use crate::error::{ErrorKind, RBError};

use rusoto_s3::{ListObjectsV2Request, S3Client, S3};

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
        let result = self
            .client
            .list_buckets()
            .await
            .map_err(|err| RBError::new_with_source(ErrorKind::S3, err))?;

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
        let mut params = ListObjectsV2Request {
            bucket,
            prefix: prefix.clone(),
            delimiter: Some(String::from("/")),
            continuation_token: None,
            encoding_type: None,
            fetch_owner: None,
            max_keys: None,
            request_payer: None,
            start_after: None,
        };

        let mut results: Vec<String> = Vec::new();
        let mut files: Vec<String> = Vec::new();

        loop {
            let output = self
                .client
                .list_objects_v2(params.clone())
                .await
                .map_err(|err| RBError::new_with_source(ErrorKind::S3, err))?;

            if let Some(prefixes) = output.common_prefixes {
                results.extend(prefixes.into_iter().filter_map(|object| object.prefix));
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
                                        Some(key_no_prefix.to_string())
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

            // It's convenient to not use if let Some() here because params.continuation_token is also an Option
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
}
