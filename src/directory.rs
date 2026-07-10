use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub trait Directory: Send + Sync{
    fn exists(&self, path: &Path) -> bool;
    fn read(&self, path: &Path) -> crate::Result<Vec<u8>>;
    fn write(&self, path: &Path, data: &[u8]) -> crate::Result<()>;
}


#[derive(Debug)]
pub struct FsDirectory{
    root: PathBuf
}


impl FsDirectory{
    pub fn open<T: Into<PathBuf>>(root: T) -> crate::Result<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;
        Ok(Self{root})
    }
}

impl Directory for FsDirectory{
    fn exists(&self, path: &Path) -> bool {
        self.root.join(path).exists()
    }

    fn read(&self, path: &Path) -> crate::Result<Vec<u8>> {
        let mut file = std::fs::File::open(self.root.join(path))?;
        let mut raw = Vec::new();
        file.read_to_end(&mut raw)?;
        Ok(raw)
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
}