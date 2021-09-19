mod git;
use std::{fs::create_dir_all, path::{Path, PathBuf}};

use generator::{generate, DIRECTORIES};
use git::Repo;

use anyhow::*;
fn main() -> Result<()> {
    let (template, destination): (String, PathBuf) = {
        let mut args = std::env::args().skip(1);
        (
            args.next().context("Missing template URL")?,
            args.next().context("Missing destination path")?.into(),
        )
    };
    if destination.exists() {
        bail!("Destination path exists")
    }
    let caches = DIRECTORIES.cache_dir();
    if !caches.exists() {
        create_dir_all(&caches)?
    }
    let cached_path = caches.join(&template);
    let template = if <str as AsRef<Path>>::as_ref(&template).exists() {
        template.into()
    } else {
        if !cached_path.exists() {
            Repo::clone(&template, &cached_path)?;
        } else {
            Repo::open(&cached_path)?.pull()?
        }
        cached_path
    };
    generate(template, destination)?;
    Ok(())
}
