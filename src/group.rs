//! Turning a pile of hashes into groups of "these are the same picture".
//!
//! The first version of this file used union-find: if A is within the threshold
//! of B, and B of C, all three became one group. That is single-linkage
//! clustering, and on a real folder of 31 662 frames it produced one group of
//! **912 files whose extremes were 39 bits apart** - completely different
//! pictures chained together through a path of small steps.
//!
//! So grouping is complete-linkage instead: a file joins a group only if it is
//! within the threshold of *every* member. A group is therefore guaranteed to
//! contain nothing further apart than the threshold, which is the promise the
//! output makes to whoever is about to delete files.

use std::collections::HashMap;

use crate::hash::distance;

/// One image as far as grouping is concerned.
#[derive(Debug, Clone)]
pub struct Entry {
    pub path: String,
    pub hash: u64,
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
}

impl Entry {
    pub fn pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}

/// A set of images that are the same picture, best copy first.
#[derive(Debug)]
pub struct Group {
    pub entries: Vec<Entry>,
    pub max_distance: u32,
}

impl Group {
    /// The copy worth keeping: most pixels, then largest file.
    pub fn keeper(&self) -> &Entry {
        &self.entries[0]
    }

    /// What the others cost in disk space.
    pub fn reclaimable_bytes(&self) -> u64 {
        self.entries.iter().skip(1).map(|entry| entry.bytes).sum()
    }

    pub fn is_exact(&self) -> bool {
        self.max_distance == 0
    }
}

/// Better copy first: resolution wins, file size breaks the tie, path settles it.
fn better(a: &Entry, b: &Entry) -> std::cmp::Ordering {
    b.pixels()
        .cmp(&a.pixels())
        .then(b.bytes.cmp(&a.bytes))
        .then(a.path.cmp(&b.path))
}

/// Group entries that are within `threshold` bits of each other.
///
/// Identical hashes are collapsed first, so the O(n^2) part runs over *unique*
/// hashes - on the 31k-file folder that is a fraction of the input, and the
/// whole grouping step takes under a second.
pub fn group(entries: &[Entry], threshold: u32) -> Vec<Group> {
    // 1. exact matches: cheap, and they shrink the expensive step below.
    let mut by_hash: HashMap<u64, Vec<Entry>> = HashMap::new();
    for entry in entries {
        by_hash.entry(entry.hash).or_default().push(entry.clone());
    }

    let mut unique: Vec<u64> = by_hash.keys().copied().collect();
    // Deterministic output regardless of directory order.
    unique.sort_unstable();

    // 2. complete-linkage clustering over the unique hashes.
    let mut clusters: Vec<Vec<u64>> = Vec::new();
    for &hash in &unique {
        let mut joined = false;
        for cluster in clusters.iter_mut() {
            if cluster.iter().all(|&member| distance(member, hash) <= threshold) {
                cluster.push(hash);
                joined = true;
                break;
            }
        }
        if !joined {
            clusters.push(vec![hash]);
        }
    }

    // 3. expand back into files.
    let mut groups: Vec<Group> = clusters
        .into_iter()
        .filter_map(|cluster| {
            let mut members: Vec<Entry> = cluster
                .iter()
                .flat_map(|hash| by_hash.get(hash).cloned().unwrap_or_default())
                .collect();
            if members.len() < 2 {
                return None;         // one file is not a duplicate of anything
            }
            members.sort_by(better);

            let mut max_distance = 0;
            for (position, a) in members.iter().enumerate() {
                for b in members.iter().skip(position + 1) {
                    max_distance = max_distance.max(distance(a.hash, b.hash));
                }
            }
            Some(Group { entries: members, max_distance })
        })
        .collect();

    // Biggest win first.
    groups.sort_by(|a, b| {
        b.reclaimable_bytes()
            .cmp(&a.reclaimable_bytes())
            .then(a.keeper().path.cmp(&b.keeper().path))
    });
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, hash: u64, bytes: u64, width: u32, height: u32) -> Entry {
        Entry { path: path.into(), hash, bytes, width, height }
    }

    #[test]
    fn identical_hashes_form_one_group() {
        let entries = vec![
            entry("a.jpg", 0xff00, 100, 800, 600),
            entry("b.jpg", 0xff00, 90, 800, 600),
            entry("c.jpg", 0x00ff, 80, 800, 600),
        ];
        let groups = group(&entries, 0);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].entries.len(), 2);
        assert!(groups[0].is_exact());
    }

    #[test]
    fn a_lone_image_is_not_a_group() {
        let entries = vec![entry("only.jpg", 1, 10, 100, 100)];
        assert!(group(&entries, 5).is_empty());
    }

    #[test]
    fn chaining_is_refused_a_group_never_exceeds_the_threshold() {
        // a-b are 4 apart, b-c are 4 apart, a-c are 8: with single linkage all
        // three would merge. They must not.
        let entries = vec![
            entry("a.jpg", 0b0000_0000, 10, 100, 100),
            entry("b.jpg", 0b0000_1111, 10, 100, 100),
            entry("c.jpg", 0b1111_1111, 10, 100, 100),
        ];
        let groups = group(&entries, 4);
        assert_eq!(groups.len(), 1, "exactly one pair should group");
        assert_eq!(groups[0].entries.len(), 2);
        assert!(groups[0].max_distance <= 4);
    }

    #[test]
    fn every_group_stays_inside_the_threshold() {
        // A ladder of hashes two bits apart each: no group may span more than 4.
        let entries: Vec<Entry> = (0..8u32)
            .map(|step| {
                let hash = (0..step).fold(0u64, |acc, bit| acc | (0b11 << (bit * 2)));
                entry(&format!("{step}.jpg"), hash, 10, 100, 100)
            })
            .collect();
        for group in group(&entries, 4) {
            assert!(group.max_distance <= 4, "group spanned {} bits", group.max_distance);
        }
    }

    #[test]
    fn the_threshold_is_respected() {
        let entries = vec![
            entry("a.jpg", 0b0000_0000, 10, 100, 100),
            entry("b.jpg", 0b0011_1111, 10, 100, 100),   // 6 bits away
        ];
        assert!(group(&entries, 5).is_empty());
        assert_eq!(group(&entries, 6).len(), 1);
    }

    #[test]
    fn the_highest_resolution_copy_is_kept() {
        let entries = vec![
            entry("thumb.jpg", 7, 20_000, 320, 240),
            entry("original.jpg", 7, 900_000, 3840, 2160),
            entry("medium.jpg", 7, 200_000, 1280, 720),
        ];
        let groups = group(&entries, 2);
        assert_eq!(groups[0].keeper().path, "original.jpg");
        assert_eq!(groups[0].reclaimable_bytes(), 220_000);
    }

    #[test]
    fn equal_resolution_falls_back_to_file_size() {
        let entries = vec![
            entry("compressed.jpg", 3, 50_000, 1000, 1000),
            entry("full.png", 3, 800_000, 1000, 1000),
        ];
        assert_eq!(group(&entries, 0)[0].keeper().path, "full.png");
    }

    #[test]
    fn groups_are_ordered_by_how_much_space_they_waste() {
        let entries = vec![
            entry("small_a.jpg", 1, 1_000, 100, 100),
            entry("small_b.jpg", 1, 1_000, 100, 100),
            entry("big_a.jpg", 99, 500_000, 100, 100),
            entry("big_b.jpg", 99, 500_000, 100, 100),
        ];
        let groups = group(&entries, 0);
        assert_eq!(groups.len(), 2);
        assert!(groups[0].reclaimable_bytes() > groups[1].reclaimable_bytes());
    }

    #[test]
    fn output_does_not_depend_on_input_order() {
        let mut entries = vec![
            entry("b.jpg", 0x10, 10, 100, 100),
            entry("a.jpg", 0x10, 10, 100, 100),
            entry("c.jpg", 0xff00, 10, 100, 100),
            entry("d.jpg", 0xff00, 10, 100, 100),
        ];
        let first = group(&entries, 2);
        entries.reverse();
        let second = group(&entries, 2);

        let paths = |groups: &[Group]| -> Vec<Vec<String>> {
            groups.iter().map(|g| g.entries.iter().map(|e| e.path.clone()).collect()).collect()
        };
        assert_eq!(paths(&first), paths(&second));
    }
}
