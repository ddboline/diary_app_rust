use anyhow::Error;
use aws_config::SdkConfig;
use aws_sdk_s3::{
    operation::list_objects::ListObjectsOutput,
    types::{Bucket, Object},
    Client as S3Client,
};
use bytes::Bytes;
use std::fmt;
use time::OffsetDateTime;
use tokio::io::AsyncReadExt;

use crate::exponential_retry;

#[derive(Clone)]
pub struct S3Instance {
    s3_client: S3Client,
    max_keys: Option<i32>,
}

impl fmt::Debug for S3Instance {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("S3Instance")
    }
}

impl S3Instance {
    #[must_use]
    pub fn new(sdk_config: &SdkConfig) -> Self {
        Self {
            s3_client: S3Client::from_conf(sdk_config.into()),
            max_keys: None,
        }
    }

    #[must_use]
    pub fn max_keys(mut self, max_keys: i32) -> Self {
        self.max_keys = Some(max_keys);
        self
    }

    /// # Errors
    /// Return error if s3 api fails
    pub async fn get_list_of_buckets(&self) -> Result<Vec<Bucket>, Error> {
        exponential_retry(|| async move {
            self.s3_client
                .list_buckets()
                .send()
                .await
                .map(|l| l.buckets.unwrap_or_default())
                .map_err(Into::into)
        })
        .await
    }

    /// # Errors
    /// Return error if s3 api fails
    pub async fn upload_from_string(
        &self,
        input_str: &str,
        bucket_name: &str,
        key_name: &str,
    ) -> Result<(), Error> {
        exponential_retry(|| async move {
            let body = Bytes::copy_from_slice(input_str.as_bytes()).into();
            self.s3_client
                .put_object()
                .bucket(bucket_name)
                .key(key_name)
                .body(body)
                .send()
                .await
                .map(|_| ())
                .map_err(Into::into)
        })
        .await
    }

    /// # Errors
    /// Return error if s3 api fails
    pub async fn download_to_string(
        &self,
        bucket_name: &str,
        key_name: &str,
    ) -> Result<(String, OffsetDateTime), Error> {
        exponential_retry(|| async move {
            let resp = self
                .s3_client
                .get_object()
                .bucket(bucket_name)
                .key(key_name)
                .send()
                .await?;
            let last_modified = resp
                .last_modified
                .and_then(|t| OffsetDateTime::from_unix_timestamp(t.as_secs_f64() as i64).ok())
                .unwrap_or_else(OffsetDateTime::now_utc);

            let mut buf = String::new();
            resp.body.into_async_read().read_to_string(&mut buf).await?;
            Ok((buf, last_modified))
        })
        .await
    }

    async fn list_keys(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        marker: Option<impl AsRef<str>>,
        max_keys: Option<i32>,
    ) -> Result<ListObjectsOutput, Error> {
        let mut builder = self.s3_client.list_objects().bucket(bucket);
        if let Some(prefix) = prefix {
            builder = builder.prefix(prefix);
        }
        if let Some(marker) = marker {
            builder = builder.marker(marker.as_ref());
        }
        if let Some(max_keys) = max_keys {
            builder = builder.max_keys(max_keys);
        }
        builder.send().await.map_err(Into::into)
    }

    /// # Errors
    /// Return error if s3 api fails
    pub async fn get_list_of_keys(
        &self,
        bucket: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<Object>, Error> {
        exponential_retry(|| async move {
            let mut marker: Option<String> = None;
            let mut list_of_keys = Vec::new();
            let mut max_keys = self.max_keys;
            loop {
                let mut output = self
                    .list_keys(bucket, prefix, marker.as_ref(), max_keys)
                    .await?;
                if let Some(contents) = output.contents.take() {
                    if let Some(last) = contents.last() {
                        if let Some(key) = &last.key {
                            marker.replace(key.into());
                        }
                    }
                    if let Some(n) = max_keys {
                        max_keys.replace(n - contents.len() as i32);
                    }
                    list_of_keys.extend_from_slice(&contents);
                }
                if output.is_truncated == Some(false) || output.is_truncated.is_none() {
                    break;
                }
            }
            Ok(list_of_keys)
        })
        .await
    }
}
