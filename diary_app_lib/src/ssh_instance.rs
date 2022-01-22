use anyhow::{format_err, Error};
use lazy_static::lazy_static;
use log::debug;
use smallvec::{smallvec, SmallVec};
use std::{
    collections::HashMap,
    fmt::{Display, Write},
    process::Stdio,
};
use tokio::{
    io::{stdout, AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::{Mutex, RwLock},
};
use url::Url;

use stack_string::{format_sstr, StackString};

lazy_static! {
    static ref LOCK_CACHE: RwLock<HashMap<StackString, Mutex<()>>> = RwLock::new(HashMap::new());
}

#[derive(Debug, Clone)]
pub struct SSHInstance {
    pub user: StackString,
    pub host: StackString,
    pub port: u16,
}

impl SSHInstance {
    pub async fn new(
        user: impl Into<StackString>,
        host: impl Into<StackString>,
        port: u16,
    ) -> Self {
        let host = host.into();
        LOCK_CACHE
            .write()
            .await
            .insert(host.clone(), Mutex::new(()));
        Self {
            user: user.into(),
            host,
            port,
        }
    }

    pub async fn from_url(url: &Url) -> Result<Self, Error> {
        let host = url.host_str().ok_or_else(|| format_err!("Parse error"))?;
        let port = url.port().unwrap_or(22);
        let user = url.username();
        Ok(Self::new(user, host, port).await)
    }

    pub fn get_ssh_str(&self, path: impl Display) -> StackString {
        if self.port == 22 {
            format_sstr!("{}@{}:{}", self.user, self.host, path)
        } else {
            format_sstr!("-p {} {}@{}:{}", self.port, self.user, self.host, path)
        }
    }

    pub fn get_ssh_username_host(&self) -> SmallVec<[StackString; 3]> {
        let user_host = format_sstr!("{}@{}", self.user, self.host);
        if self.port == 22 {
            smallvec![user_host]
        } else {
            let port = StackString::from_display(self.port);
            smallvec!["-p".into(), port, user_host]
        }
    }

    pub async fn run_command_stream_stdout(&self, cmd: &str) -> Result<Vec<StackString>, Error> {
        if let Some(host_lock) = LOCK_CACHE.read().await.get(&self.host) {
            let _guard = host_lock.lock().await;
            debug!("run_command_stream_stdout cmd {}", cmd);
            let user_host = self.get_ssh_username_host();
            let mut args: SmallVec<[&str; 4]> = user_host.iter().map(StackString::as_str).collect();
            args.push(cmd);
            let results = Command::new("ssh").args(&args).output().await?;
            if results.stdout.is_empty() {
                Ok(Vec::new())
            } else {
                results
                    .stdout
                    .split(|c| *c == b'\n')
                    .map(|s| StackString::from_utf8(s.to_vec()).map_err(Into::into))
                    .collect()
            }
        } else {
            Err(format_err!("Failed to acquire lock"))
        }
    }

    pub async fn run_command_print_stdout(&self, cmd: &str) -> Result<(), Error> {
        if let Some(host_lock) = LOCK_CACHE.read().await.get(&self.host) {
            let _guard = host_lock.lock();
            debug!("run_command_print_stdout cmd {}", cmd);
            let user_host = self.get_ssh_username_host();
            let mut args: SmallVec<[&str; 4]> = user_host.iter().map(StackString::as_str).collect();
            args.push(cmd);
            let mut command = Command::new("ssh")
                .args(&args)
                .stdout(Stdio::piped())
                .spawn()?;

            let stdout_handle = command
                .stdout
                .take()
                .ok_or_else(|| format_err!("No stdout"))?;
            let mut reader = BufReader::new(stdout_handle);

            let mut line = String::new();
            let mut stdout = stdout();
            while let Ok(bytes) = reader.read_line(&mut line).await {
                if bytes > 0 {
                    let user_host = &user_host[user_host.len() - 1];
                    let write_line = format_sstr!("ssh://{user_host}{line}");
                    stdout.write_all(write_line.as_bytes()).await?;
                } else {
                    break;
                }
            }
            command.wait().await?;
        }
        Ok(())
    }

    pub async fn run_command_ssh(&self, cmd: &str) -> Result<(), Error> {
        let user_host = self.get_ssh_username_host();
        let mut args: SmallVec<[&str; 4]> = user_host.iter().map(StackString::as_str).collect();
        args.push(cmd);
        if let Some(host_lock) = LOCK_CACHE.read().await.get(&self.host) {
            let _guard = host_lock.lock().await;
            debug!("run_command_ssh cmd {}", cmd);
            if Command::new("ssh").args(&args).status().await?.success() {
                Ok(())
            } else {
                Err(format_err!("{cmd} failed"))
            }
        } else {
            Err(format_err!("Failed to acquire lock"))
        }
    }
}
