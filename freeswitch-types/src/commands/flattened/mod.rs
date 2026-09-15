//! Dial strings the switch produced, read the way the switch reads them.
//!
//! A directory group's expansion or any other list the switch wrote is read by
//! the switch's own passes for a named [`DialStringTarget`], so each leg's
//! variables are the ones its channel receives. Every leg keeps its input text,
//! and [`FlattenedDialString::display_raw`] forwards the kept legs unchanged.

use std::fmt;
use std::sync::Arc;

use crate::channel::HangupCause;
use crate::commands::endpoint::Endpoint;
use crate::commands::variables::DialStringTarget;
use crate::variables::VariableName;
use pipeline::{Block, Leg};

pub(crate) mod pipeline;
#[cfg(test)]
mod tests;

/// A dial string read by the passes the switch applies for one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlattenedDialString {
    head: String,
    blocks: Vec<Block>,
    threads: Vec<FlattenedThread>,
    tail: String,
    warnings: Vec<ListWarning>,
}

/// One `:_:` thread, or the whole list when there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlattenedThread {
    separator: String,
    head: String,
    blocks: Vec<Block>,
    groups: Vec<FlattenedGroup>,
    trailer: String,
}

/// Legs rung together; groups of a thread are tried in turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlattenedGroup {
    separator: String,
    legs: Vec<FlattenedLeg>,
}

/// One leg as its channel receives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlattenedLeg {
    separator: String,
    raw: String,
    leg: Leg,
    inherited: Arc<[Block]>,
    target: LegTarget,
    warnings: Vec<LegWarning>,
}

/// What a leg dials.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LegTarget {
    /// `error/`, which ends the leg with a cause and places no call.
    Error(ErrorLeg),
    /// An endpoint this crate models; it carries no variable block.
    Endpoint(Endpoint),
    /// An endpoint this crate does not model, or an empty leg.
    Unparsed(UnparsedLeg),
}

/// An `error/` leg.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorLeg {
    as_written: String,
    reading: CauseReading,
}

/// Endpoint text no typed endpoint accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnparsedLeg {
    endpoint: String,
}

/// How `switch_channel_str2cause` reads the text after `error/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CauseReading {
    /// A cause name, in any case.
    Name(HangupCause),
    /// A leading digit run, read with `atoi`.
    Number(u32),
    /// Neither; the switch ends the leg with its default cause.
    Unrecognized,
}

/// A pair of a leg's own blocks that does not reach the channel as written.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LegWarning {
    /// The pair has no `=`, so nothing is installed.
    PairIgnored {
        /// Index among the leg's blocks.
        block: usize,
        /// Variable name.
        key: String,
    },
    /// The value is empty, which deletes every earlier value of the key.
    PairCleared {
        /// Index among the leg's blocks.
        block: usize,
        /// Variable name.
        key: String,
    },
    /// The value holds `${`, which the switch refuses unless
    /// `origination_nested_vars=true` appears in the list.
    NestedVarsRefused {
        /// Index among the leg's blocks.
        block: usize,
        /// Variable name.
        key: String,
    },
}

/// Something about the whole list the typed view cannot show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ListWarning {
    /// A quote kept a leg or group separator from splitting.
    QuoteSpansLegs,
    /// The dialplan carrier substitutes a `${}` reference with a value only the
    /// switch knows; the reference is kept as written.
    CarrierExpands,
}

/// What stops the switch from reading the dial string at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FlattenedDialStringError {
    /// Nothing to dial.
    Empty,
    /// The API carrier's argument split cuts the dial string into more than one
    /// argument, or leaves a quote open.
    ArgvSplit,
    /// A block never closes, which aborts the whole originate.
    UnclosedBlock {
        /// Index of the leg in reading order.
        leg: usize,
    },
    /// The text ends in `\n`, as a reply body does before its suffix is stripped.
    TrailingNewline,
}

/// Renders a [`FlattenedDialString`]. Returned by
/// [`FlattenedDialString::display_raw`] and [`FlattenedDialString::display_for`].
#[derive(Debug, Clone, Copy)]
pub struct FlattenedDialStringDisplay<'a> {
    list: &'a FlattenedDialString,
    render: Render,
}

#[derive(Debug, Clone, Copy)]
enum Render {
    Raw,
    For(DialStringTarget),
}

impl FlattenedDialString {
    /// Read `input` through every pass `target` applies.
    pub fn parse_for(
        input: &str,
        target: impl Into<DialStringTarget>,
    ) -> Result<Self, FlattenedDialStringError> {
        todo!("{input} {:?}", target.into())
    }

    /// Threads in reading order.
    pub fn threads(&self) -> impl Iterator<Item = &FlattenedThread> {
        self.threads
            .iter()
    }

    /// Every leg of every thread, in reading order.
    pub fn legs(&self) -> impl Iterator<Item = &FlattenedLeg> {
        self.threads
            .iter()
            .flat_map(FlattenedThread::legs)
    }

    /// Warnings about the whole list.
    pub fn warnings(&self) -> &[ListWarning] {
        &self.warnings
    }

    /// Keep the legs `keep` accepts. A group or thread left with no leg goes too.
    pub fn retain(&mut self, keep: impl FnMut(&FlattenedLeg) -> bool) {
        todo!("{:p}", &keep)
    }

    /// Whether no leg is left.
    pub fn is_empty(&self) -> bool {
        todo!()
    }

    /// The input text of the kept legs, joined by the separators that stood
    /// before each of them in the input; byte-identical when nothing was removed.
    pub fn display_raw(&self) -> FlattenedDialStringDisplay<'_> {
        FlattenedDialStringDisplay {
            list: self,
            render: Render::Raw,
        }
    }

    /// The list rendered by this crate for `target`, carrying only the pairs
    /// that set a value.
    pub fn display_for(
        &self,
        target: impl Into<DialStringTarget>,
    ) -> FlattenedDialStringDisplay<'_> {
        FlattenedDialStringDisplay {
            list: self,
            render: Render::For(target.into()),
        }
    }
}

impl fmt::Display for FlattenedDialStringDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl FlattenedThread {
    /// Groups in the order they are tried.
    pub fn groups(&self) -> impl Iterator<Item = &FlattenedGroup> {
        self.groups
            .iter()
    }

    /// Every leg of every group.
    pub fn legs(&self) -> impl Iterator<Item = &FlattenedLeg> {
        self.groups
            .iter()
            .flat_map(FlattenedGroup::legs)
    }
}

impl FlattenedGroup {
    /// Legs rung together.
    pub fn legs(&self) -> impl Iterator<Item = &FlattenedLeg> {
        self.legs
            .iter()
    }
}

impl FlattenedLeg {
    /// The leg's own input text.
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// The value the leg's channel receives, after the list and thread blocks
    /// are installed in the order `local_var_clobber` decides.
    ///
    /// On a channel with `CF_NO_PRESENCE`, originate deletes `presence_id`.
    pub fn variable(&self, name: impl VariableName) -> Option<&str> {
        todo!("{}", name.as_str())
    }

    /// What the leg dials.
    pub fn target(&self) -> &LegTarget {
        &self.target
    }

    /// Pairs of the leg's own blocks that do not reach the channel as written.
    pub fn warnings(&self) -> &[LegWarning] {
        &self.warnings
    }
}

impl ErrorLeg {
    /// The text after `error/`.
    pub fn as_written(&self) -> &str {
        &self.as_written
    }

    /// How the switch reads that text.
    pub fn reading(&self) -> CauseReading {
        self.reading
    }

    /// The cause named, or the numbered cause when the number fits `u16` and
    /// names one.
    ///
    /// The switch ends a zero with `DESTINATION_OUT_OF_ORDER` and an
    /// unrecognized cause with `NORMAL_CLEARING`; this crate fabricates neither.
    pub fn cause(&self) -> Option<HangupCause> {
        todo!()
    }
}

impl UnparsedLeg {
    /// The endpoint text after the leg's blocks.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

impl fmt::Display for UnparsedLeg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl fmt::Display for LegWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl fmt::Display for ListWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl fmt::Display for FlattenedDialStringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl std::error::Error for FlattenedDialStringError {}
