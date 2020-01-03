use failure::{err_msg, Error};
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
use sts_profile_auth::sts_instance::StsInstance;
use url::Url;

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
        let sts = StsInstance::new(None).expect("Failed to obtain client");
        Self {
            s3_client: sts
                .get_s3_client(Region::UsEast1)
                .expect("Failed to obtain client"),
            max_keys: None,
        }
    }
}

impl S3Instance {
    pub fn new(aws_region_name: &str) -> Self {
        let region: Region = aws_region_name.parse().ok().unwrap_or(Region::UsEast1);
        let sts = StsInstance::new(None).expect("Failed to obtain client");
        Self {
            s3_client: sts.get_s3_client(region).expect("Failed to obtain client"),
            max_keys: None,
        }
    }

    pub fn max_keys(mut self, max_keys: usize) -> Self {
        self.max_keys = Some(max_keys);
        self
    }

    pub fn get_list_of_buckets(&self) -> Result<Vec<Bucket>, Error> {
        exponential_retry(|| {
            self.s3_client
                .list_buckets()
                .sync()
                .map(|l| l.buckets.unwrap_or_default())
                .map_err(err_msg)
        })
    }

    pub fn create_bucket(&self, bucket_name: &str) -> Result<String, Error> {
        exponential_retry(|| {
            self.s3_client
                .create_bucket(CreateBucketRequest {
                    bucket: bucket_name.into(),
                    ..CreateBucketRequest::default()
                })
                .sync()?
                .location
                .ok_or_else(|| err_msg("Failed to create bucket"))
        })
    }

    pub fn delete_bucket(&self, bucket_name: &str) -> Result<(), Error> {
        exponential_retry(|| {
            self.s3_client
                .delete_bucket(DeleteBucketRequest {
                    bucket: bucket_name.into(),
                })
                .sync()
                .map_err(err_msg)
        })
    }

    pub fn delete_key(&self, bucket_name: &str, key_name: &str) -> Result<(), Error> {
        exponential_retry(|| {
            self.s3_client
                .delete_object(DeleteObjectRequest {
                    bucket: bucket_name.into(),
                    key: key_name.into(),
                    ..DeleteObjectRequest::default()
                })
                .sync()
                .map(|_| ())
                .map_err(err_msg)
        })
    }

    pub fn copy_key(
        &self,
        source: &Url,
        bucket_to: &str,
        key_to: &str,
    ) -> Result<Option<String>, Error> {
        exponential_retry(|| {
            self.s3_client
                .copy_object(CopyObjectRequest {
                    copy_source: source.to_string(),
                    bucket: bucket_to.into(),
                    key: key_to.into(),
                    ..CopyObjectRequest::default()
                })
                .sync()
                .map_err(err_msg)
        })
        .map(|x| x.copy_object_result.and_then(|s| s.e_tag))
    }

    pub fn upload(&self, fname: &str, bucket_name: &str, key_name: &str) -> Result<(), Error> {
        exponential_retry(|| {
            if !Path::new(fname).exists() {
                return Err(err_msg("File doesn't exist"));
            }
            exponential_retry(|| {
                self.s3_client
                    .upload_from_file(
                        fname,
                        PutObjectRequest {
                            bucket: bucket_name.into(),
                            key: key_name.into(),
                            ..PutObjectRequest::default()
                        },
                    )
                    .map_err(err_msg)
            })?;
            Ok(())
        })
    }

    pub fn upload_from_string(
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
            .sync()
            .map_err(err_msg)
            .map(|_| ())
    }

    pub fn download_to_string(&self, bucket_name: &str, key_name: &str) -> Result<String, Error> {
        exponential_retry(|| {
            let source = GetObjectRequest {
                bucket: bucket_name.into(),
                key: key_name.into(),
                ..GetObjectRequest::default()
            };
            let mut resp = self.s3_client.get_object(source).sync()?;
            let body = resp.body.take().ok_or_else(|| err_msg("no body"))?;

            let mut buf = String::new();
            body.into_blocking_read().read_to_string(&mut buf)?;
            Ok(buf)
        })
    }

    pub fn download(
        &self,
        bucket_name: &str,
        key_name: &str,
        fname: &str,
    ) -> Result<String, Error> {
        exponential_retry(|| {
            self.s3_client
                .download_to_file(
                    GetObjectRequest {
                        bucket: bucket_name.into(),
                        key: key_name.into(),
                        ..GetObjectRequest::default()
                    },
                    fname,
                )
                .map_err(err_msg)
                .and_then(|x| {
                    x.e_tag
                        .as_ref()
                        .map(|y| y.trim_matches('"').into())
                        .ok_or_else(|| err_msg("Failed download"))
                })
        })
    }

    pub fn get_list_of_keys(
        &self,
        bucket: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<Object>, Error> {
        let mut continuation_token = None;

        let mut list_of_keys = Vec::new();

        loop {
            let current_list = exponential_retry(|| {
                self.s3_client
                    .list_objects_v2(ListObjectsV2Request {
                        bucket: bucket.into(),
                        continuation_token: continuation_token.clone(),
                        prefix: prefix.map(Into::into),
                        ..ListObjectsV2Request::default()
                    })
                    .sync()
                    .map_err(err_msg)
            })?;

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

    pub fn process_list_of_keys<T>(
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
            let current_list = exponential_retry(|| {
                self.s3_client
                    .list_objects_v2(ListObjectsV2Request {
                        bucket: bucket.into(),
                        continuation_token: continuation_token.clone(),
                        prefix: prefix.map(Into::into),
                        ..ListObjectsV2Request::default()
                    })
                    .sync()
                    .map_err(err_msg)
            })?;

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
    fn test_list_buckets() {
        let s3_instance = S3Instance::new("us-east-1").max_keys(100);
        let blist = s3_instance.get_list_of_buckets().unwrap();
        let bucket = blist
            .get(0)
            .and_then(|b| b.name.clone())
            .unwrap_or_else(|| "".to_string());
        let klist = s3_instance.get_list_of_keys(&bucket, None).unwrap();
        writeln!(stdout().lock(), "{} {}", bucket, klist.len()).unwrap();
        assert!(klist.len() > 0);
    }
}
