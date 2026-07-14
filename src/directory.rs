use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};

pub trait Directory {
    fn exists(&self, path: &Path) -> bool;
    fn read(&self, path: &Path) -> crate::Result<Vec<u8>>;
    fn read_range(&self, path: &Path, range: Range<u64>) -> crate::Result<Vec<u8>>;
    fn write(&self, path: &Path, data: &[u8]) -> crate::Result<()>;

    fn delete(&self, path: &Path) -> crate::Result<()>;
}

#[derive(Debug)]
pub struct FsDirectory {
    root: PathBuf,
}

impl FsDirectory {
    pub fn open<T: Into<PathBuf>>(root: T) -> crate::Result<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }
}

impl Directory for FsDirectory {
    fn exists(&self, path: &Path) -> bool {
        self.root.join(path).exists()
    }

    fn read(&self, path: &Path) -> crate::Result<Vec<u8>> {
        let mut file = std::fs::File::open(self.root.join(path))?;
        let mut raw = Vec::new();
        file.read_to_end(&mut raw)?;
        Ok(raw)
    }

    fn read_range(&self, path: &Path, range: Range<u64>) -> crate::Result<Vec<u8>> {
        let mut file = std::fs::File::open(self.root.join(path))?;
        file.seek(SeekFrom::Start(range.start))?;
        let mut buf = Vec::with_capacity((range.end - range.start + 1) as usize);
        file.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn write(&self, path: &Path, data: &[u8]) -> crate::Result<()> {
        let full = self.root.join(path);
        let tmp = full.with_extension("tmp");

        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        std::fs::rename(tmp, full)?;
        Ok(())
    }

    fn delete(&self, path: &Path) -> crate::Result<()> {
        std::fs::remove_file(self.root.join(path))?;
        Ok(())
    }
}
