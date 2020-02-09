use anyhow::{format_err, Error};
use rusoto_core::Region;
use rusoto_s3::{
    Bucket, CopyObjectRequest, CreateBucketRequest, DeleteBucketRequest, DeleteObjectRequest,
    GetObjectRequest, ListObjectsV2Request, Object, PutObjectRequest, S3Client, S3,
};
use s4::S4;
use std::convert::Into;
use std::fmt;
use std::io::Read;
use std::path::Path;
use sts_profile_auth::get_client_sts;
use url::Url;

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
        self.s3_client
            .list_buckets()
            .await
            .map(|l| l.buckets.unwrap_or_default())
            .map_err(Into::into)
    }

    pub async fn create_bucket(&self, bucket_name: &str) -> Result<String, Error> {
        let req = CreateBucketRequest {
            bucket: bucket_name.into(),
            ..CreateBucketRequest::default()
        };
        self.s3_client
            .create_bucket(req)
            .await?
            .location
            .ok_or_else(|| format_err!("Failed to create bucket"))
    }

    pub async fn delete_bucket(&self, bucket_name: &str) -> Result<(), Error> {
        let req = DeleteBucketRequest {
            bucket: bucket_name.into(),
        };
        self.s3_client.delete_bucket(req).await.map_err(Into::into)
    }

    pub async fn delete_key(&self, bucket_name: &str, key_name: &str) -> Result<(), Error> {
        let req = DeleteObjectRequest {
            bucket: bucket_name.into(),
            key: key_name.into(),
            ..DeleteObjectRequest::default()
        };
        self.s3_client
            .delete_object(req)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    pub async fn copy_key(
        &self,
        source: &Url,
        bucket_to: &str,
        key_to: &str,
    ) -> Result<Option<String>, Error> {
        let req = CopyObjectRequest {
            copy_source: source.to_string(),
            bucket: bucket_to.into(),
            key: key_to.into(),
            ..CopyObjectRequest::default()
        };
        self.s3_client
            .copy_object(req)
            .await
            .map_err(Into::into)
            .map(|x| x.copy_object_result.and_then(|s| s.e_tag))
    }

    pub async fn upload(
        &self,
        fname: &str,
        bucket_name: &str,
        key_name: &str,
    ) -> Result<(), Error> {
        if !Path::new(fname).exists() {
            return Err(format_err!("File doesn't exist"));
        }
        let req = PutObjectRequest {
            bucket: bucket_name.into(),
            key: key_name.into(),
            ..PutObjectRequest::default()
        };
        self.s3_client.upload_from_file(fname, req).await?;
        Ok(())
    }

    pub async fn upload_from_string(
        &self,
        input_str: &str,
        bucket_name: &str,
        key_name: &str,
    ) -> Result<(), Error> {
        let target = PutObjectRequest {
            bucket: bucket_name.into(),
            key: key_name.into(),
            body: Some(input_str.to_string().into_bytes().into()),
            ..PutObjectRequest::default()
        };
        self.s3_client
            .put_object(target)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    pub async fn download_to_string(
        &self,
        bucket_name: &str,
        key_name: &str,
    ) -> Result<String, Error> {
        let source = GetObjectRequest {
            bucket: bucket_name.into(),
            key: key_name.into(),
            ..GetObjectRequest::default()
        };
        let mut resp = self.s3_client.get_object(source).await?;
        let body = resp.body.take().ok_or_else(|| format_err!("no body"))?;

        let mut buf = String::new();
        body.into_blocking_read().read_to_string(&mut buf)?;
        Ok(buf)
    }

    pub async fn download(
        &self,
        bucket_name: &str,
        key_name: &str,
        fname: &str,
    ) -> Result<String, Error> {
        let req = GetObjectRequest {
            bucket: bucket_name.into(),
            key: key_name.into(),
            ..GetObjectRequest::default()
        };
        self.s3_client
            .download_to_file(req, fname)
            .await
            .map_err(Into::into)
            .and_then(|x| {
                x.e_tag
                    .as_ref()
                    .map(|y| y.trim_matches('"').into())
                    .ok_or_else(|| format_err!("Failed download"))
            })
    }

    pub async fn get_list_of_keys(
        &self,
        bucket: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<Object>, Error> {
        let mut continuation_token = None;

        let mut list_of_keys = Vec::new();

        loop {
            let req = ListObjectsV2Request {
                bucket: bucket.into(),
                continuation_token: continuation_token.clone(),
                prefix: prefix.map(Into::into),
                ..ListObjectsV2Request::default()
            };
            let current_list = self.s3_client.list_objects_v2(req).await?;

            continuation_token = current_list.next_continuation_token.clone();

            match current_list.key_count {
                Some(0) | None => (),
                Some(_) => {
                    list_of_keys.extend_from_slice(&current_list.contents.unwrap_or_else(Vec::new));
                }
            };

            match &continuation_token {
                Some(_) => (),
                None => break,
            };
            if let Some(max_keys) = self.max_keys {
                if list_of_keys.len() > max_keys {
                    list_of_keys.resize(max_keys, Object::default());
                    break;
                }
            }
        }

        Ok(list_of_keys)
    }

    pub async fn process_list_of_keys<T>(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        callback: T,
    ) -> Result<(), Error>
    where
        T: Fn(&Object) -> () + Send + Sync,
    {
        let mut continuation_token = None;

        loop {
            let req = ListObjectsV2Request {
                bucket: bucket.into(),
                continuation_token: continuation_token.clone(),
                prefix: prefix.map(Into::into),
                ..ListObjectsV2Request::default()
            };
            let current_list = self.s3_client.list_objects_v2(req).await?;

            continuation_token = current_list.next_continuation_token.clone();

            match current_list.key_count {
                Some(0) | None => (),
                Some(_) => {
                    for item in current_list.contents.unwrap_or_else(Vec::new) {
                        callback(&item);
                    }
                }
            };

            match &continuation_token {
                Some(_) => (),
                None => break,
            };
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::s3_instance::S3Instance;
    use std::io::{stdout, Write};

    #[test]
    #[ignore]
    async fn test_list_buckets() -> Result<(), Error> {
        let s3_instance = S3Instance::new("us-east-1").max_keys(100);
        let blist = s3_instance.get_list_of_buckets().await?;
        let bucket = blist
            .get(0)
            .and_then(|b| b.name.clone())
            .unwrap_or_else(|| "".to_string());
        let klist = s3_instance.get_list_of_keys(&bucket, None).await?;
        writeln!(stdout().lock(), "{} {}", bucket, klist.len())?;
        assert!(klist.len() > 0);
        Ok(())
    }
}
