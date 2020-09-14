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
            prefix,
            delimiter: Some(String::from("/")),
            continuation_token: None,
            encoding_type: None,
            fetch_owner: None,
            max_keys: None,
            request_payer: None,
            start_after: None,
        };

        let mut results: Vec<String> = Vec::new();

        loop {
            let output = self
                .client
                .list_objects_v2(params.clone())
                .await
                .map_err(|err| RBError::new_with_source(ErrorKind::S3, err))?;

            // TODO: Right now this only lists "common prefixes," analogous to directories, but it won't list files
            // inside those directories. If I extend results with output.contents -> object.key instead of the code
            // below however, it'll return all files in all directories. What I want is somewhere between the two: it
            // should list all "directories" while also listing any files in the current "directory" that are not
            // contained in any of the already-listed "directories." This is probably going to involve listing the
            // common prefixes, then filtering the contents list so that objects whose `key` string includes one of the
            // common prefixes are left out.

            if let Some(prefixes) = output.common_prefixes {
                results.extend(prefixes.into_iter().filter_map(|object| object.prefix));
            }

            if output.next_continuation_token.is_some() {
                params.continuation_token = output.next_continuation_token;
            // don't break; we'll call the s3 function again with the new continuation token
            } else {
                break;
            }
        }

        Ok(results)
    }
}
