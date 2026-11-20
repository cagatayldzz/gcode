//! The file index of a project: every file that Git's ignore rules let through, with its size and time of change.
//!
//! The index is saved in a cache folder. Opening the project again reads that file and checks it with one `stat` per folder and per
//! ignore file: a folder's time of change moves when something in it is created, deleted or renamed. If nothing moved, the saved
//! index is used as it is, without walking the tree; if something did, the tree is walked again in parallel and saved.

use ignore::{WalkBuilder, WalkState};
use std::{
  fs, io,
  os::unix::ffi::{OsStrExt as _, OsStringExt as _},
  path::{Path, PathBuf},
  sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc,
  },
  time::{SystemTime, UNIX_EPOCH},
};

const MAGIC: &[u8; 4] = b"VKIX";
const VERSION: u32 = 1;

/// Numbers the temporary files of concurrent saves.
static SAVES: AtomicUsize = AtomicUsize::new(0);

/// One file of the project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
  /// The path below the project folder.
  pub path: PathBuf,
  pub size: u64,
  /// Nanoseconds since 1970 when the file last changed; 0 when the platform does not say.
  pub modified: i64,
}

/// The files of a project, in path order, and what to check to know whether they are still the files there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectIndex {
  root: PathBuf,
  files: Vec<FileEntry>,
  /// The folders and ignore files whose change means the files may have changed, with their times of change (`-1`: absent).
  stamps: Vec<(PathBuf, i64)>,
}

/// What is known about the saved index of a project, for showing it and deciding whether to rebuild it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexInfo {
  /// Where the index is kept.
  pub path: PathBuf,
  /// There is a saved index that can be read.
  pub saved: bool,
  /// The folders and ignore files are as they were when it was made, so it is used as it is.
  pub fresh: bool,
  /// How many files it lists, and how many bytes they add up to.
  pub files: usize,
  pub total_bytes: u64,
  /// The size of the saved index itself.
  pub saved_bytes: u64,
  /// When the index was saved.
  pub saved_at: Option<SystemTime>,
}

/// An index that was opened, and where it came from.
pub struct Opened {
  pub index: ProjectIndex,
  /// The saved index was still right and no walk was needed.
  pub from_cache: bool,
}

impl ProjectIndex {
  /// Walks the project and builds its index.
  pub fn scan(root: &Path) -> Self {
    let (sender, receiver) = mpsc::channel();
    WalkBuilder::new(root)
      .hidden(false)
      .git_ignore(true)
      .git_exclude(true)
      .require_git(false)
      .follow_links(false)
      .filter_entry(|entry| entry.file_name() != ".git")
      .build_parallel()
      .run(|| {
        let sender = sender.clone();
        Box::new(move |entry| {
          if let Ok(entry) = entry {
            if let Some(kind) = entry.file_type() {
              let metadata = entry.metadata().ok();
              let _ = sender.send((
                entry.into_path(),
                kind.is_dir(),
                metadata.as_ref().map_or(0, fs::Metadata::len),
                metadata.as_ref().map_or(0, modified),
              ));
            }
          }
          WalkState::Continue
        })
      });
    drop(sender);
    let mut files = Vec::new();
    let mut stamps = vec![(PathBuf::new(), stamp(root))];
    for (path, is_dir, size, modified) in receiver {
      let Ok(relative) = path.strip_prefix(root) else { continue };
      if relative.as_os_str().is_empty() {
        continue;
      }
      if is_dir {
        stamps.push((relative.to_path_buf(), modified));
      } else {
        if relative.file_name().is_some_and(|name| name == ".gitignore") {
          stamps.push((relative.to_path_buf(), modified));
        }
        files.push(FileEntry { path: relative.to_path_buf(), size, modified });
      }
    }
    // The file that Git keeps its own ignore rules in.
    let exclude = PathBuf::from(".git/info/exclude");
    let exclude_stamp = stamp(&root.join(&exclude));
    stamps.push((exclude, exclude_stamp));
    files.sort_unstable_by(|left, right| left.path.cmp(&right.path));
    stamps.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    Self { root: root.to_path_buf(), files, stamps }
  }

  /// The index of the project: the saved one if it is still right, else a new walk, saved for next time. A failure to save is
  /// not an error: the index is only a cache.
  pub fn open(cache_dir: &Path, root: &Path) -> Opened {
    if let Some(saved) = Self::load(cache_dir, root).filter(Self::is_fresh) {
      return Opened { index: saved, from_cache: true };
    }
    let index = Self::scan(root);
    let _ = index.save(cache_dir);
    Opened { index, from_cache: false }
  }

  /// Whether the folders and ignore files are as they were when the index was made.
  pub fn is_fresh(&self) -> bool {
    self.stamps.iter().all(|(path, saved)| stamp(&self.root.join(path)) == *saved)
  }

  pub fn root(&self) -> &Path {
    &self.root
  }

  /// Every file, by path below the project folder.
  pub fn files(&self) -> &[FileEntry] {
    &self.files
  }

  /// The full paths of the files, in path order.
  pub fn paths(&self) -> impl Iterator<Item = PathBuf> + '_ {
    self.files.iter().map(|file| self.root.join(&file.path))
  }

  /// What the saved index of `root` looks like now. Reads the saved file and checks it against the folders; it does not walk the tree.
  pub fn info(cache_dir: &Path, root: &Path) -> IndexInfo {
    let path = Self::cache_file(cache_dir, root);
    let metadata = fs::metadata(&path).ok();
    let saved_bytes = metadata.as_ref().map_or(0, fs::Metadata::len);
    let saved_at = metadata.and_then(|metadata| metadata.modified().ok());
    match Self::load(cache_dir, root) {
      Some(index) => IndexInfo {
        path,
        saved: true,
        fresh: index.is_fresh(),
        files: index.files.len(),
        total_bytes: index.files.iter().map(|file| file.size).sum(),
        saved_bytes,
        saved_at,
      },
      None => IndexInfo { path, saved: false, fresh: false, files: 0, total_bytes: 0, saved_bytes, saved_at },
    }
  }

  /// Walks the project again whatever the saved index says, saves the result and returns it.
  pub fn rebuild(cache_dir: &Path, root: &Path) -> io::Result<Self> {
    let index = Self::scan(root);
    index.save(cache_dir)?;
    Ok(index)
  }

  /// Deletes the saved index of `root`. Returns whether there was one. The next time the project is opened it is walked again.
  pub fn remove(cache_dir: &Path, root: &Path) -> io::Result<bool> {
    match fs::remove_file(Self::cache_file(cache_dir, root)) {
      Ok(()) => Ok(true),
      Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
      Err(error) => Err(error),
    }
  }

  /// Where the index of `root` is kept inside `cache_dir`.
  pub fn cache_file(cache_dir: &Path, root: &Path) -> PathBuf {
    cache_dir.join(format!("{:016x}.idx", fnv(root.as_os_str().as_bytes())))
  }

  pub fn save(&self, cache_dir: &Path) -> io::Result<()> {
    fs::create_dir_all(cache_dir)?;
    let mut out = Vec::with_capacity(64 + self.files.len() * 48);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    put_bytes(&mut out, self.root.as_os_str().as_bytes());
    out.extend_from_slice(&(self.stamps.len() as u32).to_le_bytes());
    for (path, time) in &self.stamps {
      put_bytes(&mut out, path.as_os_str().as_bytes());
      out.extend_from_slice(&time.to_le_bytes());
    }
    out.extend_from_slice(&(self.files.len() as u32).to_le_bytes());
    for file in &self.files {
      put_bytes(&mut out, file.path.as_os_str().as_bytes());
      out.extend_from_slice(&file.size.to_le_bytes());
      out.extend_from_slice(&file.modified.to_le_bytes());
    }
    // Written beside the file and moved over it, so that a crash never leaves half an index. The temporary name is this call's own:
    // the background refresh and a search may save the same project's index at the same time.
    let target = Self::cache_file(cache_dir, &self.root);
    let temporary = target.with_extension(format!("{}-{}.tmp", std::process::id(), SAVES.fetch_add(1, Ordering::Relaxed)));
    fs::write(&temporary, out)?;
    fs::rename(&temporary, target).inspect_err(|_| {
      let _ = fs::remove_file(&temporary);
    })
  }

  /// The saved index of `root`; `None` when there is none, it is damaged, or it belongs to another version or folder.
  pub fn load(cache_dir: &Path, root: &Path) -> Option<Self> {
    let data = fs::read(Self::cache_file(cache_dir, root)).ok()?;
    let mut input = Reader(&data);
    if input.take(4)? != MAGIC || u32::from_le_bytes(input.take(4)?.try_into().ok()?) != VERSION {
      return None;
    }
    if Path::new(&std::ffi::OsString::from_vec(input.bytes()?.to_vec())) != root {
      return None;
    }
    let stamp_count = input.u32()? as usize;
    let mut stamps = Vec::with_capacity(stamp_count.min(1 << 20));
    for _ in 0..stamp_count {
      let path = PathBuf::from(std::ffi::OsString::from_vec(input.bytes()?.to_vec()));
      stamps.push((path, input.i64()?));
    }
    let file_count = input.u32()? as usize;
    let mut files = Vec::with_capacity(file_count.min(1 << 22));
    for _ in 0..file_count {
      let path = PathBuf::from(std::ffi::OsString::from_vec(input.bytes()?.to_vec()));
      files.push(FileEntry { path, size: input.u64()?, modified: input.i64()? });
    }
    input.0.is_empty().then_some(Self { root: root.to_path_buf(), files, stamps })
  }
}

/// The time of change of a file or folder in nanoseconds since 1970; -1 when it is not there.
fn stamp(path: &Path) -> i64 {
  fs::metadata(path).map_or(-1, |metadata| modified(&metadata))
}

fn modified(metadata: &fs::Metadata) -> i64 {
  metadata.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok()).map_or(0, |time| time.as_nanos() as i64)
}

fn fnv(bytes: &[u8]) -> u64 {
  bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3))
}

fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
  out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
  out.extend_from_slice(bytes);
}

/// Reads the saved format, giving `None` instead of panicking on a short or damaged file.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
  fn take(&mut self, count: usize) -> Option<&'a [u8]> {
    let (head, tail) = self.0.split_at_checked(count)?;
    self.0 = tail;
    Some(head)
  }

  fn u32(&mut self) -> Option<u32> {
    Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
  }

  fn u64(&mut self) -> Option<u64> {
    Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
  }

  fn i64(&mut self) -> Option<i64> {
    Some(i64::from_le_bytes(self.take(8)?.try_into().ok()?))
  }

  fn bytes(&mut self) -> Option<&'a [u8]> {
    let length = self.u32()? as usize;
    self.take(length)
  }
}
