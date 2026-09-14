//! Bounded identity-scoped range cache.

use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
};

use crate::{
    error::{NcvError, Result},
    storage::object_store::{ByteRange, ObjectIdentity, RangeBlock},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CacheCategory {
    Metadata,
    Range,
    Index,
    Decoded,
    Rendered,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkingSetUsage {
    pub range_bytes: usize,
    pub index_bytes: usize,
    pub decoded_bytes: usize,
    pub rendered_bytes: usize,
    pub total_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    identity: String,
    start: u64,
    end: u64,
}

#[derive(Debug)]
struct CacheEntry {
    category: CacheCategory,
    block: RangeBlock,
}

#[derive(Debug)]
pub struct RangeCache {
    limit: usize,
    used: usize,
    entries: HashMap<CacheKey, CacheEntry>,
    lru: VecDeque<CacheKey>,
}

impl RangeCache {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            used: 0,
            entries: HashMap::new(),
            lru: VecDeque::new(),
        }
    }

    pub fn byte_usage(&self) -> usize {
        self.used
    }

    pub fn insert(&mut self, category: CacheCategory, block: RangeBlock) -> Result<()> {
        let bytes = block.bytes().len();
        if bytes > self.limit {
            return Err(NcvError::InvalidRange(format!(
                "cache entry of {bytes} bytes exceeds cache limit {}",
                self.limit
            )));
        }
        let key = key_for(&block);
        self.remove_key(&key);
        while self.used + bytes > self.limit {
            let Some((_, oldest)) = self
                .lru
                .iter()
                .enumerate()
                .filter_map(|(position, key)| {
                    self.entries
                        .get(key)
                        .map(|entry| (eviction_priority(entry.category), position, key.clone()))
                })
                .min_by_key(|(priority, position, _)| (*priority, *position))
                .map(|(_, position, key)| (position, key))
            else {
                break;
            };
            self.remove_key(&oldest);
        }
        self.used += bytes;
        self.lru.push_back(key.clone());
        self.entries.insert(key, CacheEntry { category, block });
        Ok(())
    }

    pub fn get(&mut self, identity: &ObjectIdentity, range: ByteRange) -> Option<&RangeBlock> {
        let key = CacheKey {
            identity: identity.cache_token().to_owned(),
            start: range.start(),
            end: range.end(),
        };
        if self.entries.contains_key(&key) {
            self.touch(&key);
        }
        self.entries.get(&key).map(|entry| &entry.block)
    }

    pub fn category_usage(&self, category: CacheCategory) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.category == category)
            .map(|entry| entry.block.bytes().len())
            .sum()
    }

    pub fn working_set_usage(&self) -> WorkingSetUsage {
        let range_bytes = self.category_usage(CacheCategory::Range);
        let index_bytes = self.category_usage(CacheCategory::Index)
            + self.category_usage(CacheCategory::Metadata);
        let decoded_bytes = self.category_usage(CacheCategory::Decoded);
        let rendered_bytes = self.category_usage(CacheCategory::Rendered);
        WorkingSetUsage {
            range_bytes,
            index_bytes,
            decoded_bytes,
            rendered_bytes,
            total_bytes: self.used,
        }
    }

    pub fn invalidate_identity(&mut self, identity: &str) {
        let keys: Vec<_> = self
            .entries
            .keys()
            .filter(|key| key.identity == identity)
            .cloned()
            .collect();
        for key in keys {
            self.remove_key(&key);
        }
    }

    fn touch(&mut self, key: &CacheKey) {
        if let Some(position) = self.lru.iter().position(|candidate| candidate == key) {
            self.lru.remove(position);
        }
        self.lru.push_back(key.clone());
    }

    fn remove_key(&mut self, key: &CacheKey) {
        if let Some(entry) = self.entries.remove(key) {
            self.used -= entry.block.bytes().len();
        }
        if let Some(position) = self.lru.iter().position(|candidate| candidate == key) {
            self.lru.remove(position);
        }
    }
}

fn key_for(block: &RangeBlock) -> CacheKey {
    CacheKey {
        identity: block.identity().to_owned(),
        start: block.range().start(),
        end: block.range().end(),
    }
}

fn eviction_priority(category: CacheCategory) -> u8 {
    match category {
        CacheCategory::Rendered => 0,
        CacheCategory::Decoded => 1,
        CacheCategory::Range => 2,
        CacheCategory::Index => 3,
        CacheCategory::Metadata => 4,
    }
}

pub fn coalesce_adjacent(
    mut ranges: Vec<ByteRange>,
    max_request_bytes: u64,
) -> Result<Vec<ByteRange>> {
    if max_request_bytes == 0 && !ranges.is_empty() {
        return Err(NcvError::InvalidRange(
            "maximum request size is zero".to_owned(),
        ));
    }
    ranges.sort_by_key(|range| (range.start(), range.end()));
    let mut merged = Vec::with_capacity(ranges.len());
    for range in ranges {
        let Some(previous) = merged.last_mut() else {
            merged.push(range);
            continue;
        };
        if previous.end == range.start && range.end - previous.start <= max_request_bytes {
            previous.end = range.end;
        } else {
            merged.push(range);
        }
    }
    Ok(merged)
}
