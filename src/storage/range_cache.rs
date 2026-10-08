//! Bounded identity-scoped range cache.

use std::{
    collections::{BTreeSet, HashMap},
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct CacheKey {
    identity: String,
    start: u64,
    end: u64,
}

#[derive(Debug)]
struct CacheEntry {
    category: CacheCategory,
    block: RangeBlock,
    recency: u64,
}

#[derive(Debug)]
pub struct RangeCache {
    limit: usize,
    used: usize,
    entries: HashMap<CacheKey, CacheEntry>,
    lru: BTreeSet<(u8, u64, CacheKey)>,
    next_recency: u64,
    last_hit: Option<CacheKey>,
}

impl RangeCache {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            used: 0,
            entries: HashMap::new(),
            lru: BTreeSet::new(),
            next_recency: 0,
            last_hit: None,
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
            let Some((_, _, oldest)) = self.lru.first().cloned() else {
                break;
            };
            self.remove_key(&oldest);
        }
        self.used += bytes;
        let recency = self.take_recency();
        self.lru
            .insert((eviction_priority(category), recency, key.clone()));
        self.entries.insert(
            key.clone(),
            CacheEntry {
                category,
                block,
                recency,
            },
        );
        self.last_hit = Some(key);
        Ok(())
    }

    pub fn get(&mut self, identity: &ObjectIdentity, range: ByteRange) -> Option<&RangeBlock> {
        let key = CacheKey {
            identity: identity.cache_token().to_owned(),
            start: range.start(),
            end: range.end(),
        };
        if self.entries.contains_key(&key) {
            if self.last_hit.as_ref() != Some(&key) {
                self.touch(&key);
            }
            self.last_hit = Some(key.clone());
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
        let Some((category, old_recency)) = self
            .entries
            .get(key)
            .map(|entry| (entry.category, entry.recency))
        else {
            return;
        };
        let old = (eviction_priority(category), old_recency, key.clone());
        self.lru.remove(&old);
        let recency = self.take_recency();
        self.lru
            .insert((eviction_priority(category), recency, key.clone()));
        if let Some(entry) = self.entries.get_mut(key) {
            entry.recency = recency;
        }
    }

    fn remove_key(&mut self, key: &CacheKey) {
        if let Some(entry) = self.entries.remove(key) {
            self.used -= entry.block.bytes().len();
            self.lru.remove(&(
                eviction_priority(entry.category),
                entry.recency,
                key.clone(),
            ));
            if self.last_hit.as_ref() == Some(key) {
                self.last_hit = None;
            }
        }
    }

    fn take_recency(&mut self) -> u64 {
        let value = self.next_recency;
        self.next_recency = self.next_recency.wrapping_add(1);
        value
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
