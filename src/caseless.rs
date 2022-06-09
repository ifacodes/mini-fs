//! This module contains a caseless filesystem.
//!
//! A caseless filesystem wraps an inner filesystem and treats paths as
//! case-insensitive, regardless of the case of the inner filesystem.
//!
//! Case-insensitive paths are refered to as caseless paths.
//! A caseless path that matches the real path of a file always opens that file.
//! Otherwise a caseless path will open the first path of the inner filesystem
//! that matches the caseless path.
//!
//! Internally paths are strings of type OsString, which can contain invalid
//! utf8. There is no safe way to make case-insensitive comparisons when invalid
//! utf8 is present. To minimize the effect of this restriction, the path
//! components are compared individually. Path components with valid utf8 are
//! compared in a case-insensitive way. Path components with invalid utf8 are
//! compared raw (case-sensitive).

use std::ffi::OsString;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::index::normalize_path;
use crate::prelude::*;
use crate::store::Entries;

/// Caseless filesystem wrapping an inner filesystem.
#[derive(Clone, Debug)]
pub struct CaselessFs<S> {
    /// Inner filesystem store.
    inner: S,
}

impl<S: Store> CaselessFs<S> {
    /// Creates a new caseless filesystem with the provided inner filesystem.
    /// It treats paths as case-insensitive, regardless of the case of the inner
    /// filesystem.
    pub fn new(inner: S) -> Self {
        Self { inner }
    }

    /// Moves the inner filesystem out of the caseless filesystem.
    /// Inspired by std::io::Cursor.
    pub fn into_inner(self) -> S {
        self.inner
    }

    /// Gets a reference to the inner filesystem.
    /// Inspired by std::io::Cursor.
    pub fn get_ref(&self) -> &S {
        &self.inner
    }

    /// Gets a mutable reference to the inner filesystem.
    /// Inspired by std::io::Cursor.
    pub fn get_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    /// Finds paths that match the caseless path.
    /// Path components with valid utf8 are compared in a case-insensitive way.
    /// Path components with invalid utf8 are compared raw (case-sensitive).
    pub fn find<P: AsRef<Path>>(&self, path: P) -> Vec<PathBuf> {
        let path = normalize_path(path.as_ref());
        let mut paths = vec![PathBuf::new()];
        for component in path.components() {
            paths = find_next_ascii_lowercase(&self.inner, &component, paths);
            if paths.len() == 0 {
                return paths;
            }
        }
        paths
    }
}

impl<S: Store> Store for CaselessFs<S> {
    type File = S::File;

    /// Opens the file identified by the caseless path.
    /// A caseless path that matches the real path of a file always opens that
    /// file. Otherwise a caseless path will open the first path of the
    /// inner filesystem that matches the caseless path.
    fn open_path(&self, path: &Path) -> io::Result<Self::File> {
        // real path
        if let Ok(file) = self.inner.open_path(path) {
            return Ok(file);
        }
        // caseless path
        for path in self.find(path) {
            return self.inner.open_path(&path);
        }
        Err(io::ErrorKind::NotFound.into())
    }

    /// Iterates over the entries of the inner filesystem.
    fn entries_path(&self, path: &Path) -> io::Result<Entries> {
        self.inner.entries_path(path)
    }
}

/// Finds the next path candidates.
fn find_next_ascii_lowercase<S: Store>(
    fs: &S,
    component: &Component,
    paths: Vec<PathBuf>,
) -> Vec<PathBuf> {
    let mut next = Vec::new();
    let target: OsString = match component {
        Component::Normal(os_s) => (*os_s).to_owned(),
        Component::RootDir => {
            // nothing can go before the root
            next.push(Path::new("/").to_owned());
            return next;
        }
        _ => {
            panic!("unexpected path component {:?}", component);
        }
    };
    if let Some(t_s) = target.to_str() {
        // compare utf8
        for path in paths {
            if let Ok(entries) = fs.entries(&path) {
                for e in entries {
                    if let Ok(entry) = e {
                        if let Some(e_s) = entry.name.to_str() {
                            if t_s.to_ascii_lowercase() == e_s.to_ascii_lowercase() {
                                let mut path = path.to_owned();
                                path.push(&entry.name);
                                next.push(path);
                            }
                        }
                    }
                }
            }
        }
    } else {
        // compare raw
        for path in paths {
            if let Ok(entries) = fs.entries(&path) {
                for e in entries {
                    if let Ok(entry) = e {
                        if &entry.name == &target {
                            let mut path = path.to_owned();
                            path.push(&entry.name);
                            next.push(path);
                        }
                    }
                }
            }
        }
    }
    next
}
