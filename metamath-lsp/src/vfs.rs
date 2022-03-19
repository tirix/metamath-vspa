//! Virtual File System
//! Keeps track of the files opened, and their current in-memory state

use crate::proof::ProofWorksheet;
use crate::rope_ext::read_to_rope;
use crate::rope_ext::RopeExt;
use crate::util::FileRef;
use crate::MutexExt;
use log::*;
use lsp_types::Position;
use lsp_types::TextDocumentContentChangeEvent;
use metamath_knife::Database;
use std::borrow::Cow;
use std::collections::{hash_map::Entry, HashMap};
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::{fs, io};
use xi_rope::tree::Node;
use xi_rope::{Rope, RopeInfo};

#[derive(Clone)]
pub enum FileContents {
    MMFile(Rope),
    MMPFile(ProofWorksheet),
}

impl FileContents {
    pub fn byte_to_lsp_position(&self, byte_idx: usize) -> Position {
        match self {
            FileContents::MMFile(text) => text.byte_to_lsp_position(byte_idx),
            FileContents::MMPFile(text) => text.byte_to_lsp_position(byte_idx),
        }
    }

    pub fn line(&self, line_idx: u32) -> Cow<str> {
        match self {
            FileContents::MMFile(text) => text.line(line_idx),
            FileContents::MMPFile(text) => text.line(line_idx),
        }
    }
}

pub struct VirtualFile {
    /// File data, saved (some version) or unsaved (none)
    contents: Mutex<(Option<i32>, FileContents)>,
}

impl VirtualFile {
    fn from_path(version: Option<i32>, path: PathBuf, db: &Database) -> io::Result<VirtualFile> {
        let contents = match path.extension().and_then(std::ffi::OsStr::to_str) {
            Some("mm") => {
                info!("Opening MM file {:?}", path.as_os_str());
                let file = fs::File::open(path)?;
                let text: Node<RopeInfo> = read_to_rope(file)?;
                FileContents::MMFile(text)
            }
            Some("mmp") => {
                info!("Opening MMP file {:?}", path.as_os_str());
                let file = fs::File::open(path)?;
                FileContents::MMPFile(ProofWorksheet::from_reader(file, db)?)
            }
            _ => {
                return Err(io::Error::new(ErrorKind::Unsupported, "Unknown extension"));
            }
        };
        Ok(VirtualFile {
            contents: Mutex::new((version, contents)),
        })
    }

    fn from_text(version: Option<i32>, text: String, db: &Database) -> io::Result<VirtualFile> {
        info!("Opening MMP file from text");
        let contents = FileContents::MMPFile(
            ProofWorksheet::from_string(&text, db)
                .map_err(|e| io::Error::new(ErrorKind::InvalidInput, e))?,
        );
        // let contents = FileContents::MM(text.into());
        Ok(VirtualFile {
            contents: Mutex::new((version, contents)),
        })
    }

    pub fn apply_changes(&self, new_version: i32, changes: Vec<TextDocumentContentChangeEvent>) {
        let (version, contents) = &mut *self.contents.ulock();
        *version = Some(new_version);
        match contents {
            FileContents::MMFile(_text) => {}
            FileContents::MMPFile(text) => text.apply_changes(changes),
        }
    }
}

#[derive(Default)]
pub struct Vfs(Mutex<HashMap<FileRef, Arc<VirtualFile>>>);

impl Vfs {
    pub fn get(&self, path: &FileRef) -> Option<Arc<VirtualFile>> {
        self.0.ulock().get(path).cloned()
    }

    pub fn get_or_insert(
        &self,
        path: FileRef,
        db: &Database,
    ) -> io::Result<(FileRef, Arc<VirtualFile>)> {
        match self.0.ulock().entry(path) {
            Entry::Occupied(e) => Ok((e.key().clone(), e.get().clone())),
            Entry::Vacant(e) => {
                let path = e.key().clone();
                let vf = VirtualFile::from_path(None, path.path().clone(), db)?;
                //if path.has_extension("mm") {
                //  Check if in
                //}
                let val = e.insert(Arc::new(vf)).clone();
                Ok((path, val))
            }
        }
    }

    pub fn source(&self, file: FileRef, db: &Database) -> io::Result<FileContents> {
        Ok(self.get_or_insert(file, db)?.1.contents.ulock().1.clone())
    }

    pub fn open_virt(
        &self,
        path: FileRef,
        version: i32,
        text: String,
        db: Database,
    ) -> io::Result<Arc<VirtualFile>> {
        let file = Arc::new(VirtualFile::from_text(Some(version), text, &db)?);
        let file = match self.0.ulock().entry(path) {
            Entry::Occupied(_entry) => file,
            Entry::Vacant(entry) => entry.insert(file).clone(),
        };
        Ok(file)
    }

    pub fn close(&self, path: &FileRef) {
        let mut g = self.0.ulock();
        g.remove(&path.clone());
    }
}
