//! Utilities, mainly path manipulation with some newtype definitions.

use lazy_static::lazy_static;
pub use lsp_types::{Position, Range};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::Arc;

/// Points to a specific region of a source file by identifying the region's start and end points.
#[derive(Copy, Clone, Default, PartialEq, Eq, Hash)]
pub struct Span {
    /// The byte index of the beginning of the span (inclusive).
    pub start: usize,
    /// The byte index of the end of the span (exclusive).
    pub end: usize,
}

impl From<std::ops::Range<usize>> for Span {
    #[inline]
    fn from(r: std::ops::Range<usize>) -> Self {
        Span {
            start: r.start,
            end: r.end,
        }
    }
}

impl From<std::ops::RangeInclusive<usize>> for Span {
    #[inline]
    fn from(r: std::ops::RangeInclusive<usize>) -> Self {
        Span {
            start: *r.start(),
            end: *r.end() + 1,
        }
    }
}

impl From<usize> for Span {
    #[inline]
    fn from(n: usize) -> Self {
        Span { start: n, end: n }
    }
}

impl From<Span> for std::ops::Range<usize> {
    #[inline]
    fn from(s: Span) -> Self {
        s.start..s.end
    }
}

impl Deref for Span {
    type Target = std::ops::Range<usize>;
    fn deref(&self) -> &std::ops::Range<usize> {
        unsafe { &*<*const _>::cast(self) }
    }
}

impl DerefMut for Span {
    fn deref_mut(&mut self) -> &mut std::ops::Range<usize> {
        unsafe { &mut *<*mut _>::cast(self) }
    }
}

impl IntoIterator for Span {
    type Item = usize;
    type IntoIter = std::ops::Range<usize>;
    fn into_iter(self) -> std::ops::Range<usize> {
        (*self).clone()
    }
}

impl fmt::Debug for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

lazy_static! {
  /// A [`PathBuf`] created by `lazy_static!` pointing to a canonicalized "."
  pub static ref CURRENT_DIR: PathBuf =
    std::fs::canonicalize(".").expect("failed to find current directory");
}

/// Given a [`PathBuf`] 'buf', constructs a relative path from [`CURRENT_DIR`]
/// to buf, returning it as a String.
///
/// Example: If [`CURRENT_DIR`] is `/home/johndoe/mm0`, and `buf` is
/// `/home/johndoe/Documents/ahoy.mm1` will return `../Documents/ahoy.mm1`
///
/// [`CURRENT_DIR`]: struct@CURRENT_DIR
#[cfg(not(target_arch = "wasm32"))]
fn make_relative(buf: &std::path::Path) -> String {
    pathdiff::diff_paths(buf, &*CURRENT_DIR)
        .as_deref()
        .unwrap_or(buf)
        .to_str()
        .expect("bad unicode in file path")
        .to_owned()
}

fn make_absolute(path: &str) -> PathBuf {
    std::fs::canonicalize(path).expect("Bad file path")
}

#[derive(Default)]
struct FileRefInner {
    path: PathBuf,
    rel: String,
    url: Option<lsp_types::Url>,
}

/// A reference to a file. It wraps an [`Arc`] so it can be cloned thread-safely.
/// A [`FileRef`] can be constructed either from a [`PathBuf`] or a
/// (`file://`) [`Url`](lsp_types::Url),
/// and provides (precomputed) access to these views using
/// [`path()`](FileRef::path) and [`url()`](FileRef::url), as well as
/// [`rel()`](FileRef::rel) to get the relative path from [`struct@CURRENT_DIR`].
#[derive(Clone, Default)]
pub struct FileRef(Arc<FileRefInner>);

impl From<&str> for FileRef {
    #[cfg(target_arch = "wasm32")]
    fn from(_: &str) -> FileRef {
        todo!()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn from(path: &str) -> FileRef {
        FileRef::from(make_absolute(path))
    }
}

impl From<PathBuf> for FileRef {
    #[cfg(target_arch = "wasm32")]
    fn from(_: PathBuf) -> FileRef {
        todo!()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn from(path: PathBuf) -> FileRef {
        FileRef(Arc::new(FileRefInner {
            rel: make_relative(&path),
            url: lsp_types::Url::from_file_path(std::fs::canonicalize(&path).expect("Bad path"))
                .ok(),
            path,
        }))
    }
}

impl From<lsp_types::Url> for FileRef {
    #[cfg(target_arch = "wasm32")]
    fn from(_: lsp_types::Url) -> FileRef {
        todo!()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn from(url: lsp_types::Url) -> FileRef {
        let path = url.to_file_path().expect("bad URL");
        let rel = make_relative(&path);
        FileRef(Arc::new(FileRefInner {
            path,
            rel,
            url: Some(url),
        }))
    }
}

impl FileRef {
    /// Convert this [`FileRef`] to a [`PathBuf`], for use with OS file actions.
    #[must_use]
    pub fn path(&self) -> &PathBuf {
        &self.0.path
    }

    /// Convert this [`FileRef`] to a relative path (as a `&str`).
    #[must_use]
    pub fn rel(&self) -> &str {
        &self.0.rel
    }

    /// Convert this [`FileRef`] to a `file:://` URL, for use with LSP.
    #[must_use]
    pub fn url(&self) -> &lsp_types::Url {
        self.0.url.as_ref().expect("bad file location")
    }

    /// Get a pointer to this allocation, for use in hashing.
    #[must_use]
    pub fn ptr(&self) -> *const PathBuf {
        self.path()
    }

    /// Compare this with `other` for pointer equality.
    #[must_use]
    pub fn ptr_eq(&self, other: &FileRef) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// Returns true if this file has the provided extension.
    #[must_use]
    pub fn has_extension(&self, ext: &str) -> bool {
        self.path().extension().map_or(false, |s| s == ext)
    }
}

impl PartialEq for FileRef {
    fn eq(&self, other: &Self) -> bool {
        self.0.rel == other.0.rel
    }
}

impl Eq for FileRef {}

impl Hash for FileRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.rel.hash(state)
    }
}

impl fmt::Display for FileRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = self
            .0
            .path
            .file_name()
            .unwrap_or(self.0.path.as_os_str());
        s.to_str().expect("bad unicode in path").fmt(f)
    }
}

impl fmt::Debug for FileRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// A span paired with a [`FileRef`].
#[derive(Clone, Default, PartialEq, Eq)]
pub struct FileSpan {
    /// The file in which this span occured.
    pub file: FileRef,
    /// The span (as byte indexes into the file source text).
    pub span: Span,
}

impl fmt::Debug for FileSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{:?}", self.file, self.span)
    }
}

impl<'a> From<&'a FileSpan> for Span {
    fn from(fsp: &'a FileSpan) -> Self {
        fsp.span
    }
}
