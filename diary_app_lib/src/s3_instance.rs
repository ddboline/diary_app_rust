use anyhow::{format_err, Error};
use futures::stream::{StreamExt, TryStreamExt};
use rusoto_core::Region;
use rusoto_s3::{Bucket, GetObjectRequest, Object, PutObjectRequest, S3Client, S3};
use s3_ext::S3Ext;
use std::{convert::Into, fmt};
use sts_profile_auth::get_client_sts;
use tokio::io::AsyncReadExt;

use crate::exponential_retry;

#[derive(Clone)]
pub struct S3Instance {
    s3_client: S3Client,
    max_keys: Option<usize>,
}

impl fmt::Debug for S3Instance {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "S3Instance")
    }
}

impl Default for S3Instance {
    fn default() -> Self {
        Self {
            s3_client: get_client_sts!(S3Client, Region::UsEast1).expect("Failed to obtain client"),
            max_keys: None,
        }
    }
}

impl S3Instance {
    pub fn new(aws_region_name: &str) -> Self {
        let region: Region = aws_region_name.parse().ok().unwrap_or(Region::UsEast1);
        Self {
            s3_client: get_client_sts!(S3Client, region).expect("Failed to obtain client"),
            max_keys: None,
        }
    }

    pub fn max_keys(mut self, max_keys: usize) -> Self {
        self.max_keys = Some(max_keys);
        self
    }

    pub async fn get_list_of_buckets(&self) -> Result<Vec<Bucket>, Error> {
        exponential_retry(|| async move {
            self.s3_client
                .list_buckets()
                .await
                .map(|l| l.buckets.unwrap_or_default())
                .map_err(Into::into)
        })
        .await
    }

    pub async fn upload_from_string(
        &self,
        input_str: &str,
        bucket_name: &str,
        key_name: &str,
    ) -> Result<(), Error> {
        exponential_retry(|| {
            let target = PutObjectRequest {
                bucket: bucket_name.into(),
                key: key_name.into(),
                body: Some(input_str.to_string().into_bytes().into()),
                ..PutObjectRequest::default()
            };
            async move {
                self.s3_client
                    .put_object(target)
                    .await
                    .map(|_| ())
                    .map_err(Into::into)
            }
        })
        .await
    }

    pub async fn download_to_string(
        &self,
        bucket_name: &str,
        key_name: &str,
    ) -> Result<String, Error> {
        exponential_retry(|| {
            let source = GetObjectRequest {
                bucket: bucket_name.into(),
                key: key_name.into(),
                ..GetObjectRequest::default()
            };
            async move {
                let mut resp = self.s3_client.get_object(source).await?;
                let body = resp.body.take().ok_or_else(|| format_err!("no body"))?;

                let mut buf = String::new();
                body.into_async_read().read_to_string(&mut buf).await?;
                Ok(buf)
            }
        })
        .await
    }

    pub async fn get_list_of_keys(
        &self,
        bucket: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<Object>, Error> {
        exponential_retry(|| async move {
            let stream = match prefix {
                Some(p) => self.s3_client.stream_objects_with_prefix(bucket, p),
                None => self.s3_client.stream_objects(bucket),
            };
            let results: Result<Vec<_>, _> = match self.max_keys {
                Some(nkeys) => stream.take(nkeys).try_collect().await,
                None => stream.try_collect().await,
            };
            results.map_err(Into::into)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use crate::s3_instance::S3Instance;
    use anyhow::Error;
    use log::debug;

    #[tokio::test]
    #[ignore]
    async fn test_list_buckets() -> Result<(), Error> {
        let s3_instance = S3Instance::new("us-east-1").max_keys(100);

        let bucket = s3_instance
            .get_list_of_buckets()
            .await?
            .get(0)
            .and_then(|b| b.name.clone())
            .unwrap_or_else(|| "".to_string());

        let key_list = s3_instance.get_list_of_keys(&bucket, None).await?;
        debug!("{} {}", bucket, key_list.len());
        assert!(key_list.len() > 0);
        Ok(())
    }
}
