use failure::{err_msg, Error};
use futures::Stream;
use rusoto_core::Region;
use rusoto_s3::{
    Bucket, CopyObjectRequest, CreateBucketRequest, DeleteBucketRequest, DeleteObjectRequest,
    GetObjectRequest, ListObjectsV2Request, Object, PutObjectRequest, S3Client, S3,
};
use s4::S4;
use std::fmt;
use std::path::Path;
use std::sync::Arc;
use url::Url;

use crate::exponential_retry;

#[derive(Clone)]
pub struct S3Instance {
    s3_client: Arc<S3Client>,
    max_keys: Option<usize>,
}

impl fmt::Debug for S3Instance {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Arc<S3Instance>")
    }
}

impl Default for S3Instance {
    fn default() -> Self {
        Self {
            s3_client: Arc::new(S3Client::new(Region::UsEast1)),
            max_keys: None,
        }
    }
}

impl S3Instance {
    pub fn new(aws_region_name: &str) -> Self {
        let region: Region = aws_region_name.parse().ok().unwrap_or(Region::UsEast1);
        Self {
            s3_client: Arc::new(S3Client::new(region)),
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
                    bucket: bucket_name.to_string(),
                    ..Default::default()
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
                    bucket: bucket_name.to_string(),
                })
                .sync()
                .map_err(err_msg)
        })
    }

    pub fn delete_key(&self, bucket_name: &str, key_name: &str) -> Result<(), Error> {
        exponential_retry(|| {
            self.s3_client
                .delete_object(DeleteObjectRequest {
                    bucket: bucket_name.to_string(),
                    key: key_name.to_string(),
                    ..Default::default()
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
            let copy_source = source.to_string();
            self.s3_client
                .copy_object(CopyObjectRequest {
                    copy_source,
                    bucket: bucket_to.to_string(),
                    key: key_to.to_string(),
                    ..Default::default()
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
                            bucket: bucket_name.to_string(),
                            key: key_name.to_string(),
                            ..Default::default()
                        },
                    )
                    .map_err(err_msg)
            })?;
            Ok(())
        })
    }

    pub fn download_to_string(&self, bucket_name: &str, key_name: &str) -> Result<String, Error> {
        exponential_retry(|| {
            let source = GetObjectRequest {
                bucket: bucket_name.to_string(),
                key: key_name.to_string(),
                ..Default::default()
            };
            let mut resp = self.s3_client.get_object(source).sync()?;
            let body = resp.body.take().expect("no body");

            let mut buf = Vec::new();
            let src = body.take(512 * 1024).wait();
            for chunk in src {
                buf.push(String::from_utf8_lossy(chunk?.as_ref()).into_owned());
            }
            Ok(buf.join(""))
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
                        bucket: bucket_name.to_string(),
                        key: key_name.to_string(),
                        ..Default::default()
                    },
                    fname,
                )
                .map(|x| {
                    x.e_tag
                        .as_ref()
                        .map(|y| y.trim_matches('"'))
                        .unwrap_or("")
                        .to_string()
                })
                .map_err(err_msg)
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
                        bucket: bucket.to_string(),
                        continuation_token: continuation_token.clone(),
                        prefix: prefix.map(ToString::to_string),
                        ..Default::default()
                    })
                    .sync()
                    .map_err(err_msg)
            })?;

            continuation_token = current_list.next_continuation_token.clone();

            match current_list.key_count {
                Some(0) => (),
                Some(_) => {
                    list_of_keys.extend_from_slice(&current_list.contents.unwrap_or_else(Vec::new));
                }
                None => (),
            };

            match &continuation_token {
                Some(_) => (),
                None => break,
            };
            if let Some(max_keys) = self.max_keys {
                if list_of_keys.len() > max_keys {
                    list_of_keys.resize(max_keys, Default::default());
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
                        bucket: bucket.to_string(),
                        continuation_token: continuation_token.clone(),
                        prefix: prefix.map(ToString::to_string),
                        ..Default::default()
                    })
                    .sync()
                    .map_err(err_msg)
            })?;

            continuation_token = current_list.next_continuation_token.clone();

            match current_list.key_count {
                Some(0) => (),
                Some(_) => {
                    for item in current_list.contents.unwrap_or_else(Vec::new) {
                        callback(&item);
                    }
                }
                None => (),
            };

            match &continuation_token {
                Some(_) => (),
                None => break,
            };
        }

        Ok(())
    }
}

// fn copy<W>(src: &mut StreamingBody, dest: &mut W) -> Result<(), Error>
// where
//     W: Write,
// {
//     let src = src.take(512 * 1024).wait();
//     for chunk in src {
//         dest.write_all(chunk?.as_ref())?;
//     }
//     Ok(())
// }

#[cfg(test)]
mod tests {
    use crate::s3_instance::S3Instance;

    #[test]
    fn test_list_buckets() {
        let s3_instance = S3Instance::new("us-east-1").max_keys(100);
        let blist = s3_instance.get_list_of_buckets().unwrap();
        let bucket = blist
            .get(0)
            .and_then(|b| b.name.clone())
            .unwrap_or_else(|| "".to_string());
        let klist = s3_instance.get_list_of_keys(&bucket, None).unwrap();
        println!("{} {}", bucket, klist.len());
        assert!(klist.len() > 0);
    }
}
