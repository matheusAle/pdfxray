//! The files opened lately, kept in the app data directory (`recents.json`) so the empty state
//! can offer them again. [`data_dir`] is the directory the other saved settings share.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// How many files are remembered.
pub const MAX_RECENTS: usize = 10;

/// Name of the file in the data directory.
pub const FILE_NAME: &str = "recents.json";

/// The app data directory: `$PDFXRAY_DATA_DIR`, else the platform's per-user data directory (the
/// one veraPDF is installed under). The recent files and the tag colours are saved in it.
pub fn data_dir() -> Option<PathBuf> {
    pdfxray_verapdf::default_install_dir()
        .as_deref()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
}

/// Seconds since 1970, what [`Recent::opened_at`] and the app's clock count in.
pub type Clock = fn() -> u64;

/// The real clock.
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// A file that was opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recent {
    /// Where the file is.
    pub path: PathBuf,
    /// File name, as the document reports it.
    pub name: String,
    /// Number of pages.
    pub pages: usize,
    /// Whether it has a structure tree.
    pub tagged: bool,
    /// When it was opened, in seconds since 1970 (UTC).
    pub opened_at: u64,
    /// The file was not there the last time the list was read; not saved.
    #[serde(skip)]
    pub missing: bool,
}

/// What went wrong with the saved list.
#[derive(Debug, thiserror::Error)]
pub enum RecentsError {
    /// The saved list is there but cannot be read; the next save replaces it.
    #[error("cannot read {}", path.display())]
    Read {
        /// The saved list.
        path: PathBuf,
        /// What the file system said.
        #[source]
        source: io::Error,
    },
    /// The saved list is not one this app wrote; the next save replaces it.
    #[error("{} is not a list of recent files", path.display())]
    Parse {
        /// The saved list.
        path: PathBuf,
        /// Where the JSON went wrong.
        #[source]
        source: serde_json::Error,
    },
    /// The list could not be saved.
    #[error(transparent)]
    Save(io::Error),
}

/// The saved list as it is on disk.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    files: Vec<Recent>,
}

/// The recent files, newest first, and where they are saved.
#[derive(Debug, Default)]
pub struct Recents {
    file: Option<PathBuf>,
    entries: Vec<Recent>,
    home: Option<PathBuf>,
    load_error: Option<RecentsError>,
}

impl Recents {
    /// The list in the app data directory, see [`data_dir`].
    pub fn in_data_dir() -> Self {
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
        Self::at(data_dir().as_deref()).with_home(home)
    }

    /// Writes folders under `home` as `~/…` in the list.
    #[must_use]
    pub fn with_home(mut self, home: Option<PathBuf>) -> Self {
        self.home = home;
        self
    }

    /// The home directory the folders are shortened to, if known.
    pub fn home(&self) -> Option<&Path> {
        self.home.as_deref()
    }

    /// The list saved in `dir`, or a list that is kept in memory only without one. A missing or
    /// unreadable file is an empty list: the next [`Recents::record`] replaces it. Why an existing
    /// file could not be read is kept in [`Recents::load_error`].
    pub fn at(dir: Option<&Path>) -> Self {
        let file = dir.map(|dir| dir.join(FILE_NAME));
        let (mut entries, load_error) = match file.as_deref().map(load) {
            Some(Ok(entries)) => (entries, None),
            Some(Err(error)) => (Vec::new(), Some(error)),
            None => (Vec::new(), None),
        };
        entries.truncate(MAX_RECENTS);
        mark_missing(&mut entries);
        Self {
            file,
            entries,
            home: None,
            load_error,
        }
    }

    /// Why the saved list could not be read, until a save replaces it.
    pub fn load_error(&self) -> Option<&RecentsError> {
        self.load_error.as_ref()
    }

    /// The files, newest first.
    pub fn entries(&self) -> &[Recent] {
        &self.entries
    }

    /// Puts `recent` first (a file already in the list moves up), keeps [`MAX_RECENTS`] and saves.
    /// The list is updated even when saving fails.
    pub fn record(&mut self, recent: Recent) -> Result<(), RecentsError> {
        self.entries.retain(|entry| entry.path != recent.path);
        self.entries.insert(0, recent);
        self.entries.truncate(MAX_RECENTS);
        mark_missing(&mut self.entries);
        self.save()
    }

    /// Forgets every file (File › Open Recent › Clear Menu) and saves the empty list.
    pub fn clear(&mut self) -> Result<(), RecentsError> {
        self.entries.clear();
        self.save()
    }

    fn save(&mut self) -> Result<(), RecentsError> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        let saved = Saved {
            files: self.entries.clone(),
        };
        write_list(file, &saved).map_err(RecentsError::Save)?;
        // The file that could not be read is gone.
        self.load_error = None;
        Ok(())
    }
}

/// The entries saved in `file`; a missing file has none.
fn load(file: &Path) -> Result<Vec<Recent>, RecentsError> {
    let bytes = match std::fs::read(file) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(RecentsError::Read {
                path: file.to_path_buf(),
                source,
            });
        }
    };
    serde_json::from_slice::<Saved>(&bytes)
        .map(|saved| saved.files)
        .map_err(|source| RecentsError::Parse {
            path: file.to_path_buf(),
            source,
        })
}

fn write_list(file: &Path, saved: &Saved) -> io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec_pretty(saved).map_err(io::Error::other)?;
    std::fs::write(file, json)
}

fn mark_missing(entries: &mut [Recent]) {
    for entry in entries {
        entry.missing = !entry.path.exists();
    }
}

/// `~/Documents/reports`: the folder of `path`, with `home` written as `~`.
pub fn folder_text(path: &Path, home: Option<&Path>) -> String {
    let folder = path.parent().unwrap_or(path);
    match home.and_then(|home| folder.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => folder.display().to_string(),
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Year, month (1 to 12) and day of the proleptic Gregorian calendar for `days` since 1970-01-01.
fn civil_date(days: u64) -> (i64, usize, u32) {
    let z = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = u32::try_from(day_of_year - (153 * shifted_month + 2) / 5 + 1).unwrap_or(1);
    let month = usize::try_from(if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    })
    .unwrap_or(1);
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// How long ago `then` was, as the recent list says it: `just now`, `2 min ago`, `3 h ago`,
/// `yesterday`, then the date (`Oct 27`, with the year when it is not this one). Days are UTC.
pub fn relative_time(now: u64, then: u64) -> String {
    let elapsed = now.saturating_sub(then);
    if elapsed < 60 {
        return "just now".to_owned();
    }
    if elapsed < 3600 {
        return format!("{} min ago", elapsed / 60);
    }
    let (today, then_day) = (now / 86_400, then / 86_400);
    match today.saturating_sub(then_day) {
        0 => format!("{} h ago", elapsed / 3600),
        1 => "yesterday".to_owned(),
        _ => {
            let (year, month, day) = civil_date(then_day);
            let this_year = civil_date(today).0;
            let month = MONTHS[month - 1];
            if year == this_year {
                format!("{month} {day}")
            } else {
                format!("{month} {day}, {year}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    /// 2026-10-29 12:00:00 UTC.
    const NOW: u64 = 1_793_275_200;

    #[test]
    fn times_read_as_the_list_shows_them() {
        let ago = |seconds: u64| relative_time(NOW, NOW - seconds);

        assert_eq!(ago(5), "just now");
        assert_eq!(ago(120), "2 min ago");
        assert_eq!(ago(3 * 3600), "3 h ago");
        // Noon: 13 hours back is still today, 13 hours of the day before is yesterday.
        assert_eq!(ago(11 * 3600), "11 h ago");
        assert_eq!(ago(13 * 3600), "yesterday");
        assert_eq!(ago(2 * 86_400), "Oct 27");
        assert_eq!(ago(40 * 86_400), "Sep 19");
        // Another year gets its year; a leap day is a date like the others.
        assert_eq!(relative_time(NOW, 1_709_208_000), "Feb 29, 2024");
        assert_eq!(relative_time(NOW, 0), "Jan 1, 1970");
    }

    #[test]
    fn the_folder_is_shortened_to_the_home_directory() {
        let home = Path::new("/Users/me");

        assert_eq!(
            folder_text(Path::new("/Users/me/Documents/reports/a.pdf"), Some(home)),
            "~/Documents/reports"
        );
        assert_eq!(folder_text(Path::new("/Users/me/a.pdf"), Some(home)), "~");
        assert_eq!(folder_text(Path::new("/tmp/x/a.pdf"), Some(home)), "/tmp/x");
        assert_eq!(folder_text(Path::new("/tmp/x/a.pdf"), None), "/tmp/x");
    }

    fn recent(path: &str, opened_at: u64) -> Recent {
        Recent {
            path: PathBuf::from(path),
            name: path.rsplit('/').next().unwrap_or(path).to_owned(),
            pages: 1,
            tagged: false,
            opened_at,
            missing: true,
        }
    }

    #[test]
    fn the_list_keeps_the_newest_first_without_repeats_and_at_most_ten() {
        let mut recents = Recents::default();

        for n in 0..12 {
            recents.record(recent(&format!("/tmp/{n}.pdf"), n)).unwrap();
        }
        recents.record(recent("/tmp/5.pdf", 99)).unwrap();

        let paths: Vec<String> = recents
            .entries()
            .iter()
            .map(|entry| entry.path.display().to_string())
            .collect();
        assert_eq!(paths.len(), MAX_RECENTS);
        assert_eq!(paths[..3], ["/tmp/5.pdf", "/tmp/11.pdf", "/tmp/10.pdf"]);
        assert_eq!(paths.iter().filter(|path| *path == "/tmp/5.pdf").count(), 1);
    }

    #[test]
    fn a_corrupt_file_reads_as_an_empty_list_and_says_why_until_it_is_replaced() {
        let dir = std::env::temp_dir().join(format!("pdfxray-recents-corrupt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE_NAME), "{ not json").unwrap();

        let mut recents = Recents::at(Some(&dir));
        let error = recents.load_error().map(|e| crate::state::describe(e));
        recents.record(recent("/tmp/a.pdf", 1)).unwrap();
        let after_save = recents.load_error().map(|e| crate::state::describe(e));
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(recents.entries().len(), 1);
        let file = dir.join(FILE_NAME);
        assert_eq!(
            error,
            Some(format!(
                "{} is not a list of recent files: key must be a string at line 1 column 3",
                file.display()
            ))
        );
        assert!(
            Recents::at(Some(&dir)).load_error().is_none(),
            "a missing file is no error"
        );
        // smoke: saving replaced the unreadable file, so there is nothing left to report
        assert_eq!(after_save, None);
    }

    fn recents_error_path(recents: &Recents) -> Option<&Path> {
        match recents.load_error()? {
            RecentsError::Read { path, .. } | RecentsError::Parse { path, .. } => Some(path),
            RecentsError::Save(_) => None,
        }
    }

    #[test]
    fn a_list_that_cannot_be_read_says_which_file() {
        let dir = std::env::temp_dir().join(format!("pdfxray-recents-unreadable-{}", std::process::id()));
        // A folder where the file should be: reading it fails, and not because it is missing.
        std::fs::create_dir_all(dir.join(FILE_NAME)).unwrap();

        let recents = Recents::at(Some(&dir));
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(recents_error_path(&recents), Some(dir.join(FILE_NAME).as_path()));
        assert!(
            matches!(recents.load_error(), Some(RecentsError::Read { .. })),
            "{:?}",
            recents.load_error()
        );
        // smoke: the list itself is empty
        assert_eq!(recents.entries(), []);
    }

    #[test]
    fn clearing_forgets_every_file_on_disk_too() {
        let dir = std::env::temp_dir().join(format!("pdfxray-recents-clear-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut recents = Recents::at(Some(&dir));
        recents.record(recent("/tmp/a.pdf", 1)).unwrap();
        recents.record(recent("/tmp/b.pdf", 2)).unwrap();

        recents.clear().unwrap();
        let reread = Recents::at(Some(&dir));
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(recents.entries(), []);
        // smoke: the saved list is empty as well
        assert_eq!(reread.entries().len(), 0);
    }
}
