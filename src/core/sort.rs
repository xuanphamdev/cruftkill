//! Sort comparators for displayed scan results.
//!
//! Port of npkill's `FOLDER_SORT` (`src/constants/sort.result.ts`):
//! - `Path`: lexicographic ascending
//! - `Size`: largest first; tiebreak by path ascending; `None` sizes sort to end
//! - `Age`: oldest mtime first; tiebreak by path ascending; `None` mtimes sort to end

use std::cmp::Ordering;

use crate::core::types::{FolderResult, SortBy};

/// Sort `items` in place by the given criterion.
pub fn sort_results(items: &mut [FolderResult], by: SortBy) {
    items.sort_by(|a, b| compare(a, b, by));
}

fn compare(a: &FolderResult, b: &FolderResult, by: SortBy) -> Ordering {
    match by {
        SortBy::Path => a.path.cmp(&b.path),

        SortBy::Size => match (a.size_bytes, b.size_bytes) {
            // None size sorts last (UI will not have a number to display anyway).
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (None, None) => a.path.cmp(&b.path),
            (Some(x), Some(y)) if x == y => a.path.cmp(&b.path),
            (Some(x), Some(y)) => y.cmp(&x), // desc
        },

        SortBy::Age => match (a.last_modified, b.last_modified) {
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (None, None) => a.path.cmp(&b.path),
            (Some(x), Some(y)) if x == y => a.path.cmp(&b.path),
            (Some(x), Some(y)) => x.cmp(&y), // asc (oldest first)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::ScanFoundFolder;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    fn r(path: &str, size: Option<u64>, age_secs_ago: Option<u64>) -> FolderResult {
        let mut f = FolderResult::from_scan(ScanFoundFolder::new(PathBuf::from(path), None));
        f.size_bytes = size;
        f.last_modified = age_secs_ago.map(|s| SystemTime::now() - Duration::from_secs(s));
        f
    }

    fn paths(items: &[FolderResult]) -> Vec<String> {
        items.iter().map(|f| f.path.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn sorts_by_path_ascending() {
        let mut v = vec![r("/c", None, None), r("/a", None, None), r("/b", None, None)];
        sort_results(&mut v, SortBy::Path);
        assert_eq!(paths(&v), vec!["/a", "/b", "/c"]);
    }

    #[test]
    fn sorts_by_size_desc_then_path_asc() {
        let mut v = vec![
            r("/small", Some(100), None),
            r("/big", Some(10_000), None),
            r("/big2", Some(10_000), None),
        ];
        sort_results(&mut v, SortBy::Size);
        assert_eq!(paths(&v), vec!["/big", "/big2", "/small"]);
    }

    #[test]
    fn unknown_size_sorts_last() {
        let mut v = vec![r("/a", None, None), r("/b", Some(50), None), r("/c", Some(200), None)];
        sort_results(&mut v, SortBy::Size);
        assert_eq!(paths(&v), vec!["/c", "/b", "/a"]);
    }

    #[test]
    fn sorts_by_age_oldest_first() {
        let mut v = vec![
            r("/recent", None, Some(10)),
            r("/old", None, Some(10_000)),
            r("/ancient", None, Some(100_000)),
        ];
        sort_results(&mut v, SortBy::Age);
        assert_eq!(paths(&v), vec!["/ancient", "/old", "/recent"]);
    }

    #[test]
    fn unknown_age_sorts_last_under_age() {
        let mut v = vec![
            r("/no-mtime", None, None),
            r("/has-mtime-old", None, Some(10_000)),
            r("/has-mtime-new", None, Some(1)),
        ];
        sort_results(&mut v, SortBy::Age);
        assert_eq!(paths(&v), vec!["/has-mtime-old", "/has-mtime-new", "/no-mtime"]);
    }

    #[test]
    fn equal_sizes_tiebreak_by_path() {
        let mut v = vec![
            r("/zzz", Some(100), None),
            r("/aaa", Some(100), None),
            r("/mmm", Some(100), None),
        ];
        sort_results(&mut v, SortBy::Size);
        assert_eq!(paths(&v), vec!["/aaa", "/mmm", "/zzz"]);
    }

    #[test]
    fn empty_slice_is_a_noop() {
        let mut v: Vec<FolderResult> = vec![];
        sort_results(&mut v, SortBy::Size);
        assert!(v.is_empty());
    }
}
