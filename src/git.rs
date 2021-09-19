use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::*;
use thiserror::Error;

pub struct Repo(PathBuf);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("Failed to clone repo {0} to {1}")]
pub struct CloneError(String, PathBuf);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("Failed to open repo at {0}")]
pub struct OpenError(PathBuf);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("Failed to pull from remote in repo {0}")]
pub struct PullError(PathBuf);

impl Repo {
    pub fn clone(remote: &str, dst: impl AsRef<Path>) -> Result<Self> {
        let dst = dst.as_ref().to_owned();
        let out = Command::new("git")
            .arg("clone")
            .arg(remote)
            .arg(&dst)
            .output()
            .with_context(|| CloneError(remote.to_owned(), dst.clone()))?;
        ensure!(out.status.success(), CloneError(remote.to_owned(), dst));
        Ok(Self(dst))
    }

    pub fn open(location: impl AsRef<Path>) -> Result<Self> {
        let location = location.as_ref().to_owned();
        let out = Command::new("git")
            .arg("-C")
            .arg(&location)
            .arg("status")
            .output()
            .with_context(|| OpenError(location.clone()))?;
        ensure!(out.status.success(), OpenError(location));
        Ok(Self(location))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn pull(&mut self) -> Result<()> {
        let out = Command::new("git")
            .arg("-C")
            .arg(self.path())
            .arg("pull")
            .output()
            .with_context(|| PullError(self.path().to_owned()))?;
        ensure!(out.status.success(), PullError(self.path().to_owned()));
        Ok(())
    }
}
