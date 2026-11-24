//! Typeahead matching buffer following Radix UI / React Aria semantics.
//!
//! - Typing distinct characters within the reset window (`0.7s` default) accumulates a
//!   case-insensitive prefix (`"sa"` matches `"Save"`).
//! - Repeatedly pressing the same single character (`"s"`, `"s"`, `"s"`) cycles through
//!   subsequent enabled items starting with that character starting after `current_index`.
//! - Pausing longer than `timeout_secs` resets the buffer.

/// Configurable typeahead prefix buffer.
#[derive(Clone, Debug)]
pub struct TypeaheadBuffer {
    buffer: String,
    last_at: f64,
    timeout_secs: f64,
}

impl Default for TypeaheadBuffer {
    fn default() -> Self {
        Self {
            buffer: String::new(),
            last_at: f64::NEG_INFINITY,
            timeout_secs: 0.7,
        }
    }
}

impl TypeaheadBuffer {
    pub fn new(timeout_secs: f64) -> Self {
        Self {
            buffer: String::new(),
            last_at: f64::NEG_INFINITY,
            timeout_secs: timeout_secs.max(0.05),
        }
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.last_at = f64::NEG_INFINITY;
    }

    pub fn current_prefix(&self) -> &str {
        &self.buffer
    }

    /// Feeds typed `text` at timestamp `now` (seconds) and resolves the next matching enabled
    /// item index from `items` (`(label, disabled)`).
    pub fn push_and_match<'a>(
        &mut self,
        text: &str,
        now: f64,
        current: Option<usize>,
        items: impl IntoIterator<Item = (&'a str, bool)>,
    ) -> Option<usize> {
        let ch = text.chars().next()?;
        if ch.is_control() || (self.buffer.is_empty() && ch.is_whitespace()) {
            return None;
        }
        if now - self.last_at > self.timeout_secs {
            self.buffer.clear();
        }
        self.last_at = now;
        let lower: String = text.to_lowercase();
        let list: Vec<(&'a str, bool)> = items.into_iter().collect();
        if list.is_empty() {
            return None;
        }

        let is_repeated_char = !self.buffer.is_empty()
            && self.buffer.chars().all(|c| lower.chars().all(|n| c == n))
            && lower.chars().count() == 1;

        if is_repeated_char {
            let start = current.map_or(0, |i| (i + 1) % list.len());
            return find_prefix(&list, &lower, start);
        }

        self.buffer.push_str(&lower);
        let start = if self.buffer.chars().count() == 1 {
            current.map_or(0, |i| (i + 1) % list.len())
        } else {
            current.unwrap_or(0) % list.len()
        };

        if let Some(hit) = find_prefix(&list, &self.buffer, start) {
            return Some(hit);
        }

        if self.buffer.chars().count() > 1 {
            self.buffer = lower;
            let next_start = current.map_or(0, |i| (i + 1) % list.len());
            return find_prefix(&list, &self.buffer, next_start);
        }
        None
    }
}

fn find_prefix(list: &[(&str, bool)], prefix: &str, start: usize) -> Option<usize> {
    let n = list.len();
    for offset in 0..n {
        let idx = (start + offset) % n;
        let (label, disabled) = list[idx];
        if !disabled && label.to_lowercase().starts_with(prefix) {
            return Some(idx);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typeahead_accumulates_prefix_and_resets_after_pause() {
        let items = [
            ("Apple", false),
            ("Apricot", false),
            ("Banana", false),
            ("Avocado", true),
        ];
        let mut ta = TypeaheadBuffer::default();
        assert_eq!(ta.push_and_match("a", 1.0, None, items), Some(0));
        assert_eq!(ta.push_and_match("p", 1.2, Some(0), items), Some(0));
        assert_eq!(ta.push_and_match("r", 1.3, Some(0), items), Some(1));
        assert_eq!(ta.push_and_match("b", 3.0, Some(1), items), Some(2));
    }

    #[test]
    fn typeahead_cycles_repeated_char_skipping_disabled() {
        let items = [
            ("Alpha", false),
            ("Amber", true),
            ("Apex", false),
            ("Arc", false),
        ];
        let mut ta = TypeaheadBuffer::default();
        assert_eq!(ta.push_and_match("a", 1.0, Some(0), items), Some(2));
        assert_eq!(ta.push_and_match("a", 1.2, Some(2), items), Some(3));
        assert_eq!(ta.push_and_match("a", 1.4, Some(3), items), Some(0));
    }
}
