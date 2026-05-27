//! Sort comparators for displayed scan results.
//!
//! Port of npkill's `FOLDER_SORT` (`src/constants/sort.result.ts`) extended
//! with an explicit [`SortDirection`]:
//!
//! - **Path**: lexicographic.
//! - **Size**: by `size_bytes`; rows with `None` size sort LAST regardless of
//!   direction; tiebreak by path ascending regardless of direction.
//! - **Age**: by `last_modified`; same null/tiebreak rules as Size.
//!
//! Only the primary key is reversed when direction = Desc. Tiebreaks stay
//! path-ascending so output is deterministic and easy to scan.

use std::cmp::Ordering;

use crate::core::types::{FolderResult, SortBy, SortDirection};

/// Sort `items` in place.
pub fn sort_results(items: &mut [FolderResult], by: SortBy, direction: SortDirection) {
    items.sort_by(|a, b| compare(a, b, by, direction));
}

fn compare(a: &FolderResult, b: &FolderResult, by: SortBy, direction: SortDirection) -> Ordering {
    match by {
        SortBy::Path => apply_dir(a.path.cmp(&b.path), direction),

        SortBy::Size => match (a.size_bytes, b.size_bytes) {
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (None, None) => a.path.cmp(&b.path),
            (Some(x), Some(y)) if x == y => a.path.cmp(&b.path),
            (Some(x), Some(y)) => apply_dir(x.cmp(&y), direction),
        },

        SortBy::Age => match (a.last_modified, b.last_modified) {
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (None, None) => a.path.cmp(&b.path),
            (Some(x), Some(y)) if x == y => a.path.cmp(&b.path),
            (Some(x), Some(y)) => apply_dir(x.cmp(&y), direction),
        },
    }
}

#[inline]
fn apply_dir(ord: Ordering, d: SortDirection) -> Ordering {
    match d {
        SortDirection::Asc => ord,
        SortDirection::Desc => ord.reverse(),
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
    fn path_asc() {
        let mut v = vec![r("/c", None, None), r("/a", None, None), r("/b", None, None)];
        sort_results(&mut v, SortBy::Path, SortDirection::Asc);
        assert_eq!(paths(&v), vec!["/a", "/b", "/c"]);
    }

    #[test]
    fn path_desc() {
        let mut v = vec![r("/a", None, None), r("/c", None, None), r("/b", None, None)];
        sort_results(&mut v, SortBy::Path, SortDirection::Desc);
        assert_eq!(paths(&v), vec!["/c", "/b", "/a"]);
    }

    #[test]
    fn size_desc_largest_first_path_asc_tiebreak() {
        let mut v = vec![
            r("/small", Some(100), None),
            r("/big-b", Some(10_000), None),
            r("/big-a", Some(10_000), None),
        ];
        sort_results(&mut v, SortBy::Size, SortDirection::Desc);
        assert_eq!(paths(&v), vec!["/big-a", "/big-b", "/small"]);
    }

    #[test]
    fn size_asc_smallest_first_path_asc_tiebreak() {
        let mut v = vec![
            r("/big", Some(10_000), None),
            r("/small-b", Some(100), None),
            r("/small-a", Some(100), None),
        ];
        sort_results(&mut v, SortBy::Size, SortDirection::Asc);
        assert_eq!(paths(&v), vec!["/small-a", "/small-b", "/big"]);
    }

    #[test]
    fn unknown_size_sorts_last_in_both_directions() {
        let mut v_desc =
            vec![r("/a", None, None), r("/b", Some(50), None), r("/c", Some(200), None)];
        sort_results(&mut v_desc, SortBy::Size, SortDirection::Desc);
        assert_eq!(paths(&v_desc), vec!["/c", "/b", "/a"]);

        let mut v_asc =
            vec![r("/a", None, None), r("/b", Some(50), None), r("/c", Some(200), None)];
        sort_results(&mut v_asc, SortBy::Size, SortDirection::Asc);
        assert_eq!(paths(&v_asc), vec!["/b", "/c", "/a"]);
    }

    #[test]
    fn age_asc_oldest_first() {
        let mut v = vec![
            r("/recent", None, Some(10)),
            r("/old", None, Some(10_000)),
            r("/ancient", None, Some(100_000)),
        ];
        sort_results(&mut v, SortBy::Age, SortDirection::Asc);
        assert_eq!(paths(&v), vec!["/ancient", "/old", "/recent"]);
    }

    #[test]
    fn age_desc_newest_first() {
        let mut v = vec![
            r("/recent", None, Some(10)),
            r("/old", None, Some(10_000)),
            r("/ancient", None, Some(100_000)),
        ];
        sort_results(&mut v, SortBy::Age, SortDirection::Desc);
        assert_eq!(paths(&v), vec!["/recent", "/old", "/ancient"]);
    }

    #[test]
    fn unknown_age_sorts_last_in_both_directions() {
        let mut v = vec![
            r("/no-mtime", None, None),
            r("/old", None, Some(10_000)),
            r("/new", None, Some(1)),
        ];
        sort_results(&mut v, SortBy::Age, SortDirection::Asc);
        assert_eq!(paths(&v), vec!["/old", "/new", "/no-mtime"]);

        let mut v2 = vec![
            r("/no-mtime", None, None),
            r("/old", None, Some(10_000)),
            r("/new", None, Some(1)),
        ];
        sort_results(&mut v2, SortBy::Age, SortDirection::Desc);
        assert_eq!(paths(&v2), vec!["/new", "/old", "/no-mtime"]);
    }

    #[test]
    fn empty_slice_is_a_noop() {
        let mut v: Vec<FolderResult> = vec![];
        sort_results(&mut v, SortBy::Size, SortDirection::Desc);
        assert!(v.is_empty());
    }

    #[test]
    fn direction_toggle_flips_and_indicator_is_arrow() {
        assert_eq!(SortDirection::Asc.toggle(), SortDirection::Desc);
        assert_eq!(SortDirection::Desc.toggle(), SortDirection::Asc);
        assert_eq!(SortDirection::Asc.indicator(), "↑");
        assert_eq!(SortDirection::Desc.indicator(), "↓");
    }

    #[test]
    fn default_direction_is_desc() {
        assert_eq!(SortDirection::default(), SortDirection::Desc);
    }
}
