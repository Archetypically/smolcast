//! Session-only clipboard history: payloads, exact duplicate promotion,
//! recency order, and count/byte eviction; plus `CF_UNICODETEXT` decoding.
//!
//! Windows-free and host-testable (see `docs/systemPatterns.md`
//! §7.3, §7.5). Nothing here is ever serialized.
//!
//! Contract:
//! - Newest entry is first. Payload size is `text.len()` or `rgba.len()`.
//! - An item over the per-item cap is rejected without evicting anything.
//! - An exact duplicate (hash match *and* full payload equality) moves the
//!   existing entry to the front and updates its time; no second copy is kept.
//! - After inserting, the oldest entries are evicted until both the count and
//!   total-byte limits hold. The item cap never exceeds the total cap, so a new
//!   item cannot evict itself.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};

use crate::core::dib::ImagePayload;

/// A captured clipboard payload.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Payload {
    Text(String),
    Image(ImagePayload),
}

impl Payload {
    /// Retained payload bytes (§7.5).
    pub fn byte_len(&self) -> usize {
        match self {
            Payload::Text(text) => text.len(),
            Payload::Image(image) => image.rgba.len(),
        }
    }
}

fn default_hash(payload: &Payload) -> u64 {
    let mut hasher = DefaultHasher::new();
    payload.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Session-unique, never reused.
    pub id: u64,
    pub payload: Payload,
    pub captured_at_ms: i64,
    hash: u64,
}

/// History bounds (§7.5); validated by config before reaching here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_entries: usize,
    pub max_item_bytes: usize,
    pub max_total_bytes: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum InsertOutcome {
    /// A new entry; `evicted` lists removed entry ids, oldest first.
    Inserted { id: u64, evicted: Vec<u64> },
    /// An equal payload already existed and moved to the front.
    Promoted { id: u64 },
    /// Over the per-item cap; nothing changed.
    Rejected { bytes: usize },
}

pub struct History {
    entries: VecDeque<Entry>,
    /// Candidate index only: equal hashes are always confirmed by payload equality.
    by_hash: HashMap<u64, Vec<u64>>,
    total_bytes: usize,
    next_id: u64,
    limits: Limits,
    hasher: fn(&Payload) -> u64,
}

impl History {
    pub fn new(limits: Limits) -> Self {
        Self::with_hasher(limits, default_hash)
    }

    fn with_hasher(limits: Limits, hasher: fn(&Payload) -> u64) -> Self {
        Self {
            entries: VecDeque::new(),
            by_hash: HashMap::new(),
            total_bytes: 0,
            next_id: 1,
            limits,
            hasher,
        }
    }

    pub fn insert(&mut self, payload: Payload, now_ms: i64) -> InsertOutcome {
        let bytes = payload.byte_len();
        if bytes > self.limits.max_item_bytes {
            return InsertOutcome::Rejected { bytes };
        }
        let hash = (self.hasher)(&payload);
        let duplicate = self.by_hash.get(&hash).and_then(|ids| {
            ids.iter().copied().find(|id| {
                self.position(*id)
                    .is_some_and(|pos| self.entries[pos].payload == payload)
            })
        });
        if let Some(id) = duplicate {
            // The candidate allocation is dropped; the existing one is kept.
            let pos = self.position(id).expect("indexed entries exist");
            let mut entry = self.entries.remove(pos).expect("position is in range");
            entry.captured_at_ms = now_ms;
            self.entries.push_front(entry);
            return InsertOutcome::Promoted { id };
        }

        let id = self.next_id;
        self.next_id += 1;
        self.by_hash.entry(hash).or_default().push(id);
        self.total_bytes += bytes;
        self.entries.push_front(Entry {
            id,
            payload,
            captured_at_ms: now_ms,
            hash,
        });
        let evicted = self.enforce_limits();
        InsertOutcome::Inserted { id, evicted }
    }

    /// Apply new limits (config reload); returns evicted ids, oldest first.
    pub fn set_limits(&mut self, limits: Limits) -> Vec<u64> {
        self.limits = limits;
        self.enforce_limits()
    }

    pub fn remove(&mut self, id: u64) -> bool {
        match self.position(id) {
            Some(pos) => {
                let entry = self.entries.remove(pos).expect("position is in range");
                self.forget(&entry);
                true
            }
            None => false,
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.by_hash.clear();
        self.total_bytes = 0;
    }

    /// Newest first.
    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    pub fn get(&self, id: u64) -> Option<&Entry> {
        self.position(id).map(|pos| &self.entries[pos])
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    fn position(&self, id: u64) -> Option<usize> {
        self.entries.iter().position(|e| e.id == id)
    }

    /// Remove entries captured before `cutoff_ms`; returns their ids.
    pub fn expire(&mut self, cutoff_ms: i64) -> Vec<u64> {
        // Runs on every capture and most expire nothing: leave the list be.
        if !self.entries.iter().any(|e| e.captured_at_ms < cutoff_ms) {
            return Vec::new();
        }
        let mut expired = Vec::new();
        let mut kept = VecDeque::with_capacity(self.entries.len());
        for entry in std::mem::take(&mut self.entries) {
            if entry.captured_at_ms < cutoff_ms {
                self.forget(&entry);
                expired.push(entry.id);
            } else {
                kept.push_back(entry);
            }
        }
        self.entries = kept;
        expired
    }

    fn enforce_limits(&mut self) -> Vec<u64> {
        let mut evicted = Vec::new();
        while self.entries.len() > self.limits.max_entries
            || self.total_bytes > self.limits.max_total_bytes
        {
            let Some(oldest) = self.entries.pop_back() else {
                break;
            };
            self.forget(&oldest);
            evicted.push(oldest.id);
        }
        evicted
    }

    fn forget(&mut self, entry: &Entry) {
        self.total_bytes -= entry.payload.byte_len();
        if let Some(ids) = self.by_hash.get_mut(&entry.hash) {
            ids.retain(|id| *id != entry.id);
            if ids.is_empty() {
                self.by_hash.remove(&entry.hash);
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TextError {
    /// Odd byte count or shorter than one UTF-16 unit.
    BadLength,
    /// More UTF-16 bytes than the item cap allows (checked before decoding).
    TooLarge,
    /// No UTF-16 NUL terminator inside the block.
    Unterminated,
    /// Contains an unpaired surrogate.
    InvalidUtf16,
    Empty,
}

/// Decode a `CF_UNICODETEXT` global-memory block (§7.3). `bytes` is the whole
/// `GlobalSize` region. Text is preserved exactly: CRLF and surrounding
/// whitespace are kept, and invalid UTF-16 is rejected rather than repaired.
pub fn decode_unicode_text(bytes: &[u8], item_cap: usize) -> Result<String, TextError> {
    if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
        return Err(TextError::BadLength);
    }
    let limit = item_cap
        .checked_mul(2)
        .and_then(|n| n.checked_add(2))
        .unwrap_or(usize::MAX);
    if bytes.len() > limit {
        return Err(TextError::TooLarge);
    }
    // Find the terminator and decode from the bytes directly, so the UTF-16
    // units are never copied into a Vec first.
    let units = bytes.as_chunks::<2>().0;
    let end = units
        .iter()
        .position(|pair| *pair == [0, 0])
        .ok_or(TextError::Unterminated)?;
    let text: String =
        char::decode_utf16(units[..end].iter().map(|pair| u16::from_le_bytes(*pair)))
            .collect::<Result<_, _>>()
            .map_err(|_| TextError::InvalidUtf16)?;
    if text.is_empty() {
        return Err(TextError::Empty);
    }
    if text.len() > item_cap {
        return Err(TextError::TooLarge);
    }
    Ok(text)
}

/// On-disk entry record (plan §7.2): the plaintext that the persistence layer
/// encrypts with DPAPI. Layout, little-endian:
/// `b"SCLP1"`, kind (0 text, 1 image), captured_at_ms (i64), body length
/// (u64), then the body: UTF-8 text, or width (u32), height (u32), and RGBA.
const RECORD_MAGIC: &[u8; 5] = b"SCLP1";

pub fn encode_record(payload: &Payload, captured_at_ms: i64) -> Vec<u8> {
    let (kind, body_len) = match payload {
        Payload::Text(text) => (0, text.len()),
        Payload::Image(image) => (1, image.rgba.len() + 8),
    };
    // One allocation and one copy of the payload: images reach 8 MiB.
    let mut out = Vec::with_capacity(body_len + 22);
    out.extend_from_slice(RECORD_MAGIC);
    out.push(kind);
    out.extend_from_slice(&captured_at_ms.to_le_bytes());
    out.extend_from_slice(&(body_len as u64).to_le_bytes());
    match payload {
        Payload::Text(text) => out.extend_from_slice(text.as_bytes()),
        Payload::Image(image) => {
            out.extend_from_slice(&image.width.to_le_bytes());
            out.extend_from_slice(&image.height.to_le_bytes());
            out.extend_from_slice(&image.rgba);
        }
    }
    out
}

#[derive(Debug, PartialEq, Eq)]
pub struct RecordError;

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid clipboard history record")
    }
}

impl std::error::Error for RecordError {}

/// Decode a record written by [`encode_record`]. `item_cap` rejects records
/// larger than the current per-item limit.
pub fn decode_record(bytes: &[u8], item_cap: usize) -> Result<(Payload, i64), RecordError> {
    let rest = bytes.strip_prefix(RECORD_MAGIC).ok_or(RecordError)?;
    let (&kind, rest) = rest.split_first().ok_or(RecordError)?;
    let (time, rest) = rest.split_first_chunk::<8>().ok_or(RecordError)?;
    let captured_at_ms = i64::from_le_bytes(*time);
    let (len, rest) = rest.split_first_chunk::<8>().ok_or(RecordError)?;
    if u64::from_le_bytes(*len) != rest.len() as u64 {
        return Err(RecordError);
    }
    let payload = match kind {
        0 => {
            let text = std::str::from_utf8(rest).map_err(|_| RecordError)?;
            if text.is_empty() {
                return Err(RecordError);
            }
            Payload::Text(text.to_owned())
        }
        1 => {
            let (w, rest) = rest.split_first_chunk::<4>().ok_or(RecordError)?;
            let (h, rgba) = rest.split_first_chunk::<4>().ok_or(RecordError)?;
            let (width, height) = (u32::from_le_bytes(*w), u32::from_le_bytes(*h));
            let expected = (width as u64)
                .checked_mul(height as u64)
                .and_then(|n| n.checked_mul(4))
                .ok_or(RecordError)?;
            if width == 0 || height == 0 || expected != rgba.len() as u64 {
                return Err(RecordError);
            }
            Payload::Image(ImagePayload {
                width,
                height,
                rgba: rgba.to_vec(),
            })
        }
        _ => return Err(RecordError),
    };
    if payload.byte_len() > item_cap {
        return Err(RecordError);
    }
    Ok((payload, captured_at_ms))
}

/// Re-encode text for paste-back: UTF-16LE plus one NUL.
pub fn encode_unicode_text(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(Some(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(max_entries: usize, max_item_bytes: usize, max_total_bytes: usize) -> Limits {
        Limits {
            max_entries,
            max_item_bytes,
            max_total_bytes,
        }
    }

    fn text(s: &str) -> Payload {
        Payload::Text(s.into())
    }

    fn ids(history: &History) -> Vec<u64> {
        history.iter().map(|e| e.id).collect()
    }

    #[test]
    fn count_eviction_removes_oldest_at_the_boundary() {
        let mut h = History::new(limits(3, 100, 1000));
        for (i, s) in ["a", "b", "c"].into_iter().enumerate() {
            assert!(matches!(
                h.insert(text(s), i as i64),
                InsertOutcome::Inserted { ref evicted, .. } if evicted.is_empty()
            ));
        }
        assert_eq!(ids(&h), vec![3, 2, 1]);
        assert_eq!(
            h.insert(text("d"), 4),
            InsertOutcome::Inserted {
                id: 4,
                evicted: vec![1]
            }
        );
        assert_eq!(ids(&h), vec![4, 3, 2]);
    }

    #[test]
    fn byte_eviction_removes_oldest_until_total_fits() {
        let mut h = History::new(limits(100, 10, 10));
        h.insert(text("aaaa"), 1); // 4
        h.insert(text("bbbb"), 2); // 8
        assert_eq!(h.total_bytes(), 8);
        // Exactly at the total cap: nothing evicted.
        assert!(matches!(
            h.insert(text("cc"), 3),
            InsertOutcome::Inserted { ref evicted, .. } if evicted.is_empty()
        ));
        assert_eq!(h.total_bytes(), 10);
        // A full-cap item evicts every older entry, never itself.
        assert_eq!(
            h.insert(text("dddddddddd"), 4),
            InsertOutcome::Inserted {
                id: 4,
                evicted: vec![1, 2, 3]
            }
        );
        assert_eq!(ids(&h), vec![4]);
        assert_eq!(h.total_bytes(), 10);
    }

    #[test]
    fn oversize_item_is_rejected_without_eviction() {
        let mut h = History::new(limits(2, 4, 8));
        h.insert(text("aaaa"), 1);
        h.insert(text("bbbb"), 2);
        assert_eq!(
            h.insert(text("ccccc"), 3),
            InsertOutcome::Rejected { bytes: 5 }
        );
        assert_eq!(ids(&h), vec![2, 1]);
        assert_eq!(h.total_bytes(), 8);
    }

    #[test]
    fn duplicate_promotes_one_entry_and_updates_time() {
        let mut h = History::new(limits(10, 100, 1000));
        h.insert(text("same"), 1);
        h.insert(text("other"), 2);
        assert_eq!(h.insert(text("same"), 3), InsertOutcome::Promoted { id: 1 });
        assert_eq!(ids(&h), vec![1, 2]);
        assert_eq!(h.len(), 2);
        assert_eq!(h.total_bytes(), "same".len() + "other".len());
        assert_eq!(h.get(1).unwrap().captured_at_ms, 3);

        let image = |px: u8| {
            Payload::Image(ImagePayload {
                width: 1,
                height: 1,
                rgba: vec![px, 0, 0, 255],
            })
        };
        h.insert(image(9), 4);
        assert!(matches!(
            h.insert(image(9), 5),
            InsertOutcome::Promoted { .. }
        ));
        assert!(matches!(
            h.insert(image(8), 6),
            InsertOutcome::Inserted { .. }
        ));
    }

    #[test]
    fn hash_collision_still_compares_payloads() {
        let mut h = History::with_hasher(limits(10, 100, 1000), |_| 42);
        h.insert(text("one"), 1);
        assert!(matches!(
            h.insert(text("two"), 2),
            InsertOutcome::Inserted { id: 2, .. }
        ));
        assert_eq!(h.insert(text("one"), 3), InsertOutcome::Promoted { id: 1 });
        assert_eq!(h.len(), 2);
        // Removing one colliding entry keeps the other findable.
        assert!(h.remove(1));
        assert_eq!(h.insert(text("two"), 4), InsertOutcome::Promoted { id: 2 });
        assert_eq!(h.total_bytes(), 3);
    }

    #[test]
    fn remove_clear_and_shrinking_limits() {
        let mut h = History::new(limits(10, 100, 1000));
        for s in ["a", "bb", "ccc"] {
            h.insert(text(s), 0);
        }
        assert!(h.remove(2));
        assert!(!h.remove(2));
        assert_eq!(h.total_bytes(), 4);
        assert_eq!(h.set_limits(limits(1, 100, 1000)), vec![1]);
        assert_eq!(ids(&h), vec![3]);
        h.clear();
        assert!(h.is_empty());
        assert_eq!(h.total_bytes(), 0);
    }

    #[test]
    fn records_round_trip_and_reject_corruption() {
        let text = text("  line\r\nnext 😀");
        let image = Payload::Image(ImagePayload {
            width: 2,
            height: 1,
            rgba: vec![1, 2, 3, 4, 5, 6, 7, 8],
        });
        for (payload, at) in [(text, -5i64), (image, 1_800_000_000_000)] {
            let bytes = encode_record(&payload, at);
            assert_eq!(decode_record(&bytes, 1024).unwrap(), (payload.clone(), at));
            // Truncation at every length fails cleanly.
            for len in 0..bytes.len() {
                assert!(decode_record(&bytes[..len], 1024).is_err(), "len {len}");
            }
            // Over the current item cap is rejected.
            assert_eq!(
                decode_record(&bytes, payload.byte_len() - 1),
                Err(RecordError)
            );
        }
        let mut bad_kind = encode_record(&Payload::Text("x".into()), 0);
        bad_kind[5] = 9;
        assert_eq!(decode_record(&bad_kind, 1024), Err(RecordError));
        let mut bad_utf8 = encode_record(&Payload::Text("x".into()), 0);
        bad_utf8.push(0xFF);
        assert_eq!(decode_record(&bad_utf8, 1024), Err(RecordError));
    }

    fn utf16(s: &str) -> Vec<u8> {
        encode_unicode_text(s)
    }

    #[test]
    fn text_decode_preserves_exact_content() {
        let original = "  line one\r\nline two\t\r\n😀 ";
        assert_eq!(
            decode_unicode_text(&utf16(original), 1024).unwrap(),
            original
        );
        // Bytes after the first NUL (allocation slack) are ignored.
        let mut padded = utf16("hi");
        padded.extend_from_slice(&[b'x', 0, 0, 0]);
        assert_eq!(decode_unicode_text(&padded, 1024).unwrap(), "hi");
    }

    #[test]
    fn text_decode_rejections() {
        assert_eq!(decode_unicode_text(&[], 10), Err(TextError::BadLength));
        assert_eq!(
            decode_unicode_text(&[b'a', 0, 0], 10),
            Err(TextError::BadLength)
        );
        assert_eq!(
            decode_unicode_text(&[b'a', 0, b'b', 0], 10),
            Err(TextError::Unterminated)
        );
        // Unpaired high surrogate.
        assert_eq!(
            decode_unicode_text(&[0x3D, 0xD8, b'a', 0, 0, 0], 10),
            Err(TextError::InvalidUtf16)
        );
        assert_eq!(decode_unicode_text(&[0, 0], 10), Err(TextError::Empty));
        // Raw size is bounded before decoding: 2 * cap + 2 bytes.
        assert_eq!(decode_unicode_text(&utf16("abc"), 3).unwrap(), "abc");
        assert_eq!(
            decode_unicode_text(&utf16("abcd"), 3),
            Err(TextError::TooLarge)
        );
        // UTF-8 length is bounded after decoding: "é" is 2 UTF-8 bytes.
        assert_eq!(
            decode_unicode_text(&utf16("éé"), 3),
            Err(TextError::TooLarge)
        );
    }
}
