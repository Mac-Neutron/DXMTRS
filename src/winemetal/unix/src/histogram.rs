//! `DXMT_WMTCMD_HISTOGRAM`: how many records of each type the command lists encode between commits.
//!
//! With the variable naming a file, every `MTLCommandBuffer_commit` appends one line to it for
//! each (encoder kind, record type) counted since the previous commit, kinds in the order
//! `render`, `compute`, `blit` and types ascending:
//!
//! ```text
//! <kind> <record type, decimal> <count>
//! ```
//!
//! Every node a list's walk visits counts, the nop head and unknown types included. A command
//! buffer's split varies with timing; the sum over a run does not, which is what a comparison
//! between two builds reads. Unset, a list's walk costs one test of a latched flag.

use std::{
    collections::BTreeMap,
    env,
    fs::{File, OpenOptions},
    io::Write,
    sync::{LazyLock, Mutex, PoisonError},
};

/// The knob: a file name, appended to.
const VARIABLE: &str = "DXMT_WMTCMD_HISTOGRAM";

/// The file, open for the process's life: `None` when the knob is unset, empty or unopenable.
///
/// A static because the file is process-wide and latched once from the environment, and the
/// commit and encode handlers reach it without an object (CONVENTIONS, State lives on an object).
static HISTOGRAM: LazyLock<Option<Mutex<Histogram<File>>>> = LazyLock::new(|| {
    let name = env::var_os(VARIABLE).filter(|n| !n.is_empty())?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(name)
        .ok()?;
    Some(Mutex::new(Histogram::new(file)))
});

/// The three command-list encoders whose records are counted.
pub enum Kind {
    /// `_MTLRenderCommandEncoder_encodeCommands`.
    Render,
    /// `_MTLComputeCommandEncoder_encodeCommands`.
    Compute,
    /// `_MTLBlitCommandEncoder_encodeCommands`.
    Blit,
}

/// The word a line starts with, by kind: the order lines are written in.
const WORDS: [&str; 3] = ["render", "compute", "blit"];

impl Kind {
    /// The kind's place in [`WORDS`].
    const fn index(&self) -> usize {
        match self {
            Self::Render => 0,
            Self::Compute => 1,
            Self::Blit => 2,
        }
    }
}

type Counts = BTreeMap<u16, u64>;

/// The counts since the last commit, and where a commit writes them.
pub struct Histogram<W: Write> {
    out: W,
    counts: [Counts; 3],
}

impl<W: Write> Histogram<W> {
    /// An empty histogram writing to `out`.
    #[must_use]
    pub const fn new(out: W) -> Self {
        Self {
            out,
            counts: [Counts::new(), Counts::new(), Counts::new()],
        }
    }

    /// Takes a finished list's counts in.
    pub fn add(&mut self, list: List) {
        let counts = &mut self.counts[list.kind.index()];
        for (record, n) in list.counts {
            *counts.entry(record).or_insert(0) += n;
        }
    }

    /// Appends a line per count since the last commit, in one write, and starts counting again.
    ///
    /// A write that fails loses the lines: this is a development aid, not a record.
    pub fn commit(&mut self) {
        let mut lines = Vec::new();
        for (word, counts) in WORDS.iter().zip(core::mem::take(&mut self.counts)) {
            for (record, n) in counts {
                let _ = writeln!(lines, "{word} {record} {n}");
            }
        }
        if !lines.is_empty() {
            let _ = self.out.write_all(&lines);
        }
    }
}

/// The records one `encodeCommands` call walked.
pub struct List {
    kind: Kind,
    counts: Counts,
}

impl List {
    /// An empty list of `kind`.
    #[must_use]
    pub const fn new(kind: Kind) -> Self {
        Self {
            kind,
            counts: Counts::new(),
        }
    }

    /// Counts one record of type `record`.
    pub fn record(&mut self, record: u16) {
        *self.counts.entry(record).or_insert(0) += 1;
    }
}

/// A list to count a call's records in, `None` while the knob is off.
#[must_use]
pub fn list(kind: Kind) -> Option<List> {
    HISTOGRAM.as_ref().map(|_| List::new(kind))
}

/// Adds a finished list to the process's counts.
pub fn finish(list: Option<List>) {
    if let (Some(list), Some(histogram)) = (list, HISTOGRAM.as_ref()) {
        histogram
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .add(list);
    }
}

/// Writes the counts since the last commit: a command buffer is being committed.
pub fn commit() {
    if let Some(histogram) = HISTOGRAM.as_ref() {
        histogram
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .commit();
    }
}

#[cfg(test)]
mod tests;
