//! Double-buffer mechanism for tick isolation between partitions.
//!
//! The double buffer holds partition outputs between ticks. During a tick,
//! partitions read from the read buffer (previous tick's outputs) and write
//! to the write buffer (current tick's outputs). At the start of each tick,
//! buffers are swapped: the write buffer becomes the read buffer and a fresh
//! write buffer is created.

use std::collections::HashMap;

/// A double-buffered store keyed by partition ID.
///
/// Enforces the tick-isolation invariant: partition A's tick N output is
/// never visible to partition B during tick N. Outputs only become readable
/// after a `swap()` call at the start of the next tick.
pub struct DoubleBuffer {
    /// The read buffer contains the previous tick's outputs.
    read: HashMap<String, toml::Value>,
    /// The write buffer accumulates the current tick's outputs.
    write: HashMap<String, toml::Value>,
}

impl DoubleBuffer {
    /// Create a new empty double buffer.
    pub fn new() -> Self {
        Self {
            read: HashMap::new(),
            write: HashMap::new(),
        }
    }

    /// Read the output that the given partition produced during the previous tick.
    ///
    /// Returns `None` if the partition did not produce output last tick
    /// (or if no swap has occurred yet).
    pub fn read(&self, partition_id: &str) -> Option<&toml::Value> {
        self.read.get(partition_id)
    }

    /// Returns all entries in the read buffer.
    pub fn read_all(&self) -> &HashMap<String, toml::Value> {
        &self.read
    }

    /// Write a value to the write buffer for the given partition.
    ///
    /// This value will become readable after the next `swap()`.
    pub fn write(&mut self, partition_id: &str, value: toml::Value) {
        self.write.insert(partition_id.to_string(), value);
    }

    /// Returns all entries in the write buffer (current tick's outputs).
    pub fn write_all(&self) -> &HashMap<String, toml::Value> {
        &self.write
    }

    /// Swap the buffers: the current write buffer becomes the read buffer,
    /// and the write buffer is cleared for the new tick.
    ///
    /// This must be called at the start of each tick, before any partition
    /// steps execute.
    pub fn swap(&mut self) {
        std::mem::swap(&mut self.read, &mut self.write);
        self.write.clear();
    }
}

impl Default for DoubleBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_not_readable_until_swap() {
        let mut buf = DoubleBuffer::new();
        buf.write("p1", toml::Value::Integer(42));
        assert!(buf.read("p1").is_none(), "write should not be visible in read before swap");
    }

    #[test]
    fn swap_makes_writes_readable() {
        let mut buf = DoubleBuffer::new();
        buf.write("p1", toml::Value::Integer(42));
        buf.swap();
        assert_eq!(buf.read("p1"), Some(&toml::Value::Integer(42)));
    }

    #[test]
    fn swap_clears_write_buffer() {
        let mut buf = DoubleBuffer::new();
        buf.write("p1", toml::Value::Integer(1));
        buf.swap();
        assert!(buf.write_all().is_empty(), "write buffer should be empty after swap");
    }

    #[test]
    fn second_swap_replaces_previous_read() {
        let mut buf = DoubleBuffer::new();
        buf.write("p1", toml::Value::Integer(1));
        buf.swap();
        buf.write("p1", toml::Value::Integer(2));
        buf.swap();
        assert_eq!(buf.read("p1"), Some(&toml::Value::Integer(2)));
    }
}
