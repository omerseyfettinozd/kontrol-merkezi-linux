//! Rooted filesystem access. `R9T_SYSFS_ROOT=/tmp/fake` maps `/sys/...` to `/tmp/fake/sys/...`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::{HalError, Result};

#[derive(Debug, Clone)]
pub struct Sysfs {
    root: PathBuf,
}

impl Default for Sysfs {
    fn default() -> Self {
        Self::from_env()
    }
}

impl Sysfs {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn from_env() -> Self {
        Self::new(std::env::var_os("R9T_SYSFS_ROOT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/")))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn is_real(&self) -> bool {
        self.root == Path::new("/")
    }

    /// Maps an absolute path such as `/sys/class/leds` below the root.
    pub fn path(&self, abs: impl AsRef<Path>) -> PathBuf {
        let abs = abs.as_ref();
        self.root.join(abs.strip_prefix("/").unwrap_or(abs))
    }

    pub fn exists(&self, abs: impl AsRef<Path>) -> bool {
        self.path(abs).exists()
    }

    pub fn read(&self, abs: impl AsRef<Path>) -> Result<String> {
        let p = self.path(abs);
        fs::read_to_string(&p).map(|s| s.trim().to_owned()).map_err(|e| HalError::io(p, e))
    }

    pub fn parse<T: FromStr>(&self, abs: impl AsRef<Path>) -> Result<T> {
        let abs = abs.as_ref();
        let text = self.read(abs)?;
        text.parse().map_err(|_| HalError::Invalid(format!("{}: {text:?}", abs.display())))
    }

    /// One `write` call with O_TRUNC, like `echo value > file`.
    pub fn write(&self, abs: impl AsRef<Path>, value: &str) -> Result<()> {
        let p = self.path(abs);
        let mut f = fs::OpenOptions::new().write(true).truncate(true).open(&p).map_err(|e| HalError::io(&p, e))?;
        f.write_all(value.as_bytes()).map_err(|e| HalError::io(&p, e))
    }

    /// Entries of a directory (sorted file names), empty if it does not exist.
    pub fn list(&self, abs: impl AsRef<Path>) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.path(abs))
            .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        names.sort();
        names
    }

    /// Final component of a symlink target (e.g. a driver name), if resolvable.
    pub fn link_name(&self, abs: impl AsRef<Path>) -> Option<String> {
        fs::read_link(self.path(abs)).ok()?.file_name().map(|n| n.to_string_lossy().into_owned())
    }

    /// Canonical target of a symlink, as an absolute path below the root.
    pub fn resolve(&self, abs: impl AsRef<Path>) -> Option<PathBuf> {
        let real = fs::canonicalize(self.path(abs)).ok()?;
        let root = fs::canonicalize(&self.root).ok()?;
        real.strip_prefix(&root).ok().map(|p| Path::new("/").join(p))
    }
}

#[cfg(test)]
pub(crate) mod fake {
    //! Helpers to build a fake sysfs tree in a temporary directory.
    use std::fs;
    use std::path::Path;

    pub fn file(root: &Path, abs: &str, content: &str) {
        let p = root.join(abs.trim_start_matches('/'));
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, content).unwrap();
    }

    pub fn link(root: &Path, abs: &str, target: &str) {
        let p = root.join(abs.trim_start_matches('/'));
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(target, p).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rooted_reads_and_writes() {
        let dir = tempfile::tempdir().unwrap();
        fake::file(dir.path(), "/sys/a/b", "42\n");
        let fs = Sysfs::new(dir.path());
        assert_eq!(fs.parse::<u32>("/sys/a/b").unwrap(), 42);
        fs.write("/sys/a/b", "7").unwrap();
        assert_eq!(fs.read("/sys/a/b").unwrap(), "7");
        assert!(matches!(fs.read("/sys/none"), Err(HalError::NotFound(_))));
        assert!(fs.parse::<u32>("/sys/a").is_err());
        assert_eq!(fs.list("/sys"), vec!["a"]);
    }
}
