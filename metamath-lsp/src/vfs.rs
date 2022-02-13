//! Virtual File System
//! Keeps track of the files opened, and their current in-memory state

use crate::MutexExt;
use crate::util::FileRef;
use ropey::Rope;
use std::{fs, io};
use std::path::PathBuf;
use std::collections::{
    hash_map::Entry,
    HashMap,
};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}, Condvar};

#[derive(Clone)]
pub struct FileContents(pub Arc<Rope>);

pub struct VirtualFile {
    /// File data, saved (some version) or unsaved (none)
    text: Mutex<(Option<i32>, FileContents)>,
}

impl VirtualFile {
    fn from_path(version: Option<i32>, path: PathBuf) -> io::Result<VirtualFile> {
        let file = fs::File::open(path)?;
        let text = Rope::from_reader(file)?;
        Ok(VirtualFile {
            text: Mutex::new((version, FileContents(Arc::new(text)))),
        })
    }

    fn from_text(version: Option<i32>, text: String) -> VirtualFile {
        VirtualFile {
            text: Mutex::new((version, FileContents(Arc::new(text.into())))),
        }
    }
}

#[derive(Default)]
pub struct Vfs(Mutex<HashMap<FileRef, Arc<VirtualFile>>>);

impl Vfs {
    pub fn get(&self, path: &FileRef) -> Option<Arc<VirtualFile>> {
        self.0.ulock().get(path).cloned()
    }

    pub fn get_or_insert(&self, path: FileRef) -> io::Result<(FileRef, Arc<VirtualFile>)> {
        match self.0.ulock().entry(path) {
            Entry::Occupied(e) => Ok((e.key().clone(), e.get().clone())),
            Entry::Vacant(e) => {
                let path = e.key().clone();
                let vf = VirtualFile::from_path(None, path.path().clone())?;
                //if path.has_extension("mm") {
                //  Check if in 
                //}
                let val = e.insert(Arc::new(vf)).clone();
                Ok((path, val))
            }
        }
    }

    pub fn source(&self, file: &FileRef) -> FileContents {
        self.0.ulock().get(file).unwrap().text.ulock().1.clone()
    }

    pub fn open_virt(&self, path: FileRef, version: i32, text: String) -> io::Result<Arc<VirtualFile>> {
        let file = Arc::new(VirtualFile::from_text(Some(version), text));
        let file = match self.0.ulock().entry(path.clone()) {
            Entry::Occupied(entry) => {
                file
            }
            Entry::Vacant(entry) => entry.insert(file).clone()
        };
        Ok(file)
    }

    pub fn close(&self, path: &FileRef) {
        let mut g = self.0.ulock();
        g.remove(&path.clone());
    }
}
