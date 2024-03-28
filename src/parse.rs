/* Copyright (c) 2024, National Research Foundation (SARAO)
 *
 * Licensed under the BSD 3-Clause License (the "License"); you may not use
 * this file except in compliance with the License. You may obtain a copy
 * of the License at
 *
 *   https://opensource.org/licenses/BSD-3-Clause
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};
use std::borrow::Cow;
use thiserror::Error;

use katcp_codec_fsm::{Action, State};

use crate::message::{Message, MessageType};
use crate::tables::PARSER_TABLE;

type ParsedMessage<'data> = Message<Cow<'data, [u8]>, Cow<'data, [u8]>>;

/// Error returned from parsing.
#[derive(Error, Clone, Debug, Eq, PartialEq)]
#[error("{message:?} at character {position:?}")]
pub struct ParseError {
    message: String,
    position: usize,
}

impl ParseError {
    /// Create a new error.
    fn new(message: impl Into<String>, position: usize) -> Self {
        Self {
            message: message.into(),
            position,
        }
    }
}

/// Iterator implementation for [Parser::append].
pub struct ParseIterator<'parser, 'data>
where
    'data: 'parser,
{
    parser: &'parser mut Parser,
    data: &'data [u8],
    transient: Transient<'data>,
}

impl<'parser, 'data> Iterator for ParseIterator<'parser, 'data>
where
    'data: 'parser,
{
    type Item = Result<ParsedMessage<'data>, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        let (msg, tail) = self.parser.next_message(self.data, &mut self.transient);
        self.data = tail;
        msg
    }
}

/// Parser state that can only live as long as the iterator returned by [Parser::append].
struct Transient<'data> {
    /// Name which *replaces* [Parser::name]
    name: Cow<'data, [u8]>,
    /// Arguments to *append* to [Parser::arguments]
    arguments: Vec<Cow<'data, [u8]>>,
}

/// Message parser.
///
/// The parser accepts chunks of data from the wire (which need not be aligned
/// to message boundaries) and returns whole messages as they are parsed.
#[pyclass(module = "katcp_codec._lib")]
pub struct Parser {
    /// Current state
    state: State,
    /// Number of characters seen on the current line (claimed to `max_line_length`)
    line_length: usize,
    /// Configured maximum line length
    max_line_length: usize,
    /// Message type, or [None] if we haven't parsed it yet
    mtype: Option<MessageType>,
    /// Name (only allocated if [Parser::append] ends partway through the message)
    name: Vec<u8>,
    /// Message ID, or [None] if there isn't one or we haven't parsed one yet
    mid: Option<u32>,
    /// Fully-parsed arguments, excluding those in the current [Transient]
    arguments: Vec<Vec<u8>>,
    /// Current error, if we are in an error state
    error: Option<ParseError>,
}

/// Extend a `Cow<'_, [T]>` with new elements.
///
/// This is special-cased to borrow the elements if the [Cow] was empty.
fn extend_cow<'a>(cow: &mut Cow<'a, [u8]>, elements: &'a [u8]) {
    if cow.is_empty() {
        *cow = Cow::from(elements);
    } else {
        cow.to_mut().extend_from_slice(elements);
    }
}

impl Parser {
    /// Create a new parser.
    pub fn new(max_line_length: usize) -> Self {
        Self {
            state: State::Start,
            line_length: 0,
            max_line_length,
            mtype: None,
            name: vec![],
            mid: None,
            arguments: vec![],
            error: None,
        }
    }

    /// Number of bytes currently buffered for an incomplete line.
    ///
    /// This is capped at `Self::max_line_length`, even if a longer (overflowing)
    /// line is in progress.
    pub fn buffer_size(&self) -> usize {
        self.line_length
    }

    /// Return the parser to its initial state.
    pub fn reset(&mut self) {
        self.state = State::Start;
        self.line_length = 0;
        self.mtype = None;
        self.name.clear();
        self.mid = None;
        self.arguments.clear();
        self.error = None;
    }

    /// Signal an error at a particular position on a line.
    fn error_at(&mut self, transient: &mut Transient, message: impl Into<String>, position: usize) {
        if self.state != State::ErrorEndOfLine {
            self.state = State::Error;
        }
        if self.error.is_none() {
            self.error = Some(ParseError::new(message.into(), position));
        }
        // Free up some memory early
        self.arguments.clear();
        transient.arguments.clear();
    }

    /// Signal an error at the current position.
    fn error(&mut self, transient: &mut Transient, message: impl Into<String>) {
        self.error_at(transient, message, self.line_length + 1);
    }

    /// Return the parser and a [Transient] to their initial states.
    fn reset_transient(&mut self, transient: &mut Transient<'_>) {
        self.reset();
        transient.name = Cow::default();
        transient.arguments.clear();
    }

    /// Apply an [Action] to the parser.
    ///
    /// `position` is the number of characters that appeared on the line
    /// prior to `chunk`. On the other hand, [Parser::line_length] includes
    /// the length of the chunk.
    fn apply<'data>(
        &mut self,
        action: &Action,
        chunk: &'data [u8],
        transient: &mut Transient<'data>,
        position: usize,
    ) -> Result<Option<ParsedMessage<'data>>, ParseError> {
        match action {
            Action::SetType(mtype) => {
                self.mtype = Some(*mtype);
            }
            Action::Name => {
                extend_cow(&mut transient.name, chunk);
            }
            Action::Id => {
                // TODO: optimise this using the whole chunk at once
                for ch in chunk.iter() {
                    // Compute the update in 64-bit to detect overflow at the end
                    let mid = self.mid.unwrap_or(0) as u64;
                    let mid = mid * 10 + ((*ch - b'0') as u64);
                    if let Ok(value) = i32::try_from(mid) {
                        self.mid = Some(value as u32);
                    } else {
                        self.error_at(transient, "Message ID overflowed", position);
                        break;
                    }
                }
            }
            Action::Argument => {
                extend_cow(transient.arguments.last_mut().unwrap(), chunk);
            }
            Action::ArgumentEscaped(c) => {
                transient.arguments.last_mut().unwrap().to_mut().push(*c);
            }
            Action::ResetLineLength => {
                self.line_length = 0;
            }
            Action::Nothing => {}
            Action::Error => {
                self.error_at(transient, "Invalid character", position);
            }
        }

        match self.state {
            State::EndOfLine => {
                let arguments: Vec<_> = std::mem::take(&mut self.arguments)
                    .into_iter()
                    .map(Cow::from)
                    .chain(std::mem::take(&mut transient.arguments))
                    .collect();
                let msg = Message::new(
                    self.mtype.take().unwrap(),
                    std::mem::take(&mut transient.name),
                    self.mid,
                    arguments,
                );
                self.reset_transient(transient);
                Ok(Some(msg))
            }
            State::ErrorEndOfLine => {
                let error = self.error.take().unwrap();
                self.reset_transient(transient);
                Err(error)
            }
            _ => Ok(None),
        }
    }

    /// Consume data until new end-of-line is seen, returning the message if any.
    fn next_message<'data>(
        &mut self,
        mut data: &'data [u8],
        transient: &mut Transient<'data>,
    ) -> (
        Option<Result<ParsedMessage<'data>, ParseError>>,
        &'data [u8],
    ) {
        while !data.is_empty() {
            if self.line_length >= self.max_line_length && self.state != State::Error {
                self.error(transient, "Line too long");
            }

            let entry = &PARSER_TABLE[self.state][data[0]];
            if entry.create_argument {
                transient.arguments.push(Cow::default());
            }
            self.state = entry.state;
            let mut p = 1; // number of bytes we're consuming this round

            if let Some(fast_table) = &entry.fast_table {
                // Find a sequence that we can add in one step. First compute a cap.
                let max_len = if self.line_length >= self.max_line_length {
                    data.len() // We're already in the error state
                } else {
                    std::cmp::min(data.len(), self.max_line_length - self.line_length)
                };
                while p < max_len && fast_table[data[p]] {
                    p += 1;
                }
            }

            let position = self.line_length + 1;
            if self.line_length < self.max_line_length {
                // The max_len calculation guarantees that this won't exceed
                // max_line_length.
                self.line_length += p;
            }

            let result = self.apply(&entry.action, &data[..p], transient, position);
            data = &data[p..];

            match result {
                Ok(None) => {}
                Ok(Some(msg)) => {
                    return (Some(Ok(msg)), data);
                }
                Err(error) => {
                    return (Some(Err(error)), data);
                }
            }
        }
        // Return any leftover state to the primary parser state
        self.name = std::mem::take(&mut transient.name).into_owned();
        self.arguments.extend(
            std::mem::take(&mut transient.arguments)
                .into_iter()
                .map(|x| x.into_owned()),
        );
        (None, data)
    }

    /// Add data to the parser and return an iterator over messages that arise.
    ///
    /// The data is only consumed as a result of iteration. Dropping the
    /// iterator without fully consuming it has undefined results.
    #[must_use = "Must consume the returned iterator for anything to happen"]
    pub fn append<'parser, 'data, D>(
        &'parser mut self,
        data: &'data D,
    ) -> ParseIterator<'parser, 'data>
    where
        D: AsRef<[u8]> + ?Sized,
    {
        let mut transient = Transient {
            name: Cow::from(std::mem::take(&mut self.name)),
            arguments: Default::default(),
        };
        // If there is at least one argument in the state, transfer the last
        // one to the Transient so that it can be extended.
        if let Some(last_arg) = self.arguments.pop() {
            transient.arguments.push(Cow::from(last_arg));
        }
        ParseIterator {
            parser: self,
            data: data.as_ref(),
            transient,
        }
    }
}

#[pymethods]
impl Parser {
    #[new]
    fn py_new(max_line_length: usize) -> Self {
        Self::new(max_line_length)
    }

    // TODO: support buffer protocol?
    #[pyo3(name = "append")]
    fn py_append<'py>(&mut self, data: &Bound<'py, PyBytes>) -> PyResult<Bound<'py, PyList>> {
        let py = data.py();
        let out = PyList::empty_bound(py);
        for result in self.append(data.as_bytes()) {
            match result {
                Ok(msg) => {
                    out.append(msg)?;
                }
                Err(error) => {
                    out.append(PyValueError::new_err(error.to_string()).into_value(py))?;
                }
            }
        }
        Ok(out)
    }

    #[pyo3(name = "reset")]
    fn py_reset(&mut self) {
        self.reset();
    }

    #[getter(buffer_size)]
    fn py_buffer_size(&self) -> usize {
        self.buffer_size()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::message::MessageType::*;
    use crate::test::text_message_strategy;
    use proptest::prelude::*;
    use rstest::*;

    /// Helper macro for constructing messages for comparison
    macro_rules! msg {
        ( $mtype:expr, $name:literal, $mid:expr ) => {
            $crate::message::Message::new(
                $mtype,
                std::borrow::Cow::from($name.as_slice()),
                $mid,
                std::vec![],
            )
        };
        ( $mtype:expr, $name:literal, $mid:expr, $($arg:literal),* $(,)? ) => {
            $crate::message::Message::new(
                $mtype,
                std::borrow::Cow::from($name.as_slice()),
                $mid,
                std::vec![$( Cow::from($arg.as_slice()) ),*],
            )
        };
    }

    #[fixture]
    fn parser() -> Parser {
        Parser::new(usize::MAX)
    }

    #[rstest]
    #[case(
        b"?test simple\n",
        msg!(Request, b"test", None, b"simple"),
    )]
    #[case(
        b"!alternate\t\tseparators\t\r",
        msg!(Reply, b"alternate", None, b"separators"),
    )]
    #[case(
        b"#escapes \\@ \\t \\r \\n \\e \\\\ \\_\n",
        msg!(Inform, b"escapes", None, b"", b"\t", b"\r", b"\n", b"\x1B", b"\\", b" "),
    )]
    #[case(
        b"?no-args\n",
        msg!(Request, b"no-args", None),
    )]
    #[case(
        b"?no-args-trailing-spaces \n",
        msg!(Request, b"no-args-trailing-spaces", None),
    )]
    #[case(
        b"?mid[1234]\n",
        msg!(Request, b"mid", Some(1234)),
    )]
    #[case(
        b"?mid-trailing-spaces[1234]\t\r",
        msg!(Request, b"mid-trailing-spaces", Some(1234)),
    )]
    #[case(
        b"?mid-args[2147483647] foo bar\n",
        msg!(Request, b"mid-args", Some(2147483647), b"foo", b"bar"),
    )]
    #[case(
        b" \t\n\r?blank-lines\n\n",
        msg!(Request, b"blank-lines", None),
    )]
    fn test_simple(#[case] input: &[u8], #[case] message: ParsedMessage, mut parser: Parser) {
        let messages: Vec<_> = parser.append(input).collect();
        assert_eq!(messages.as_slice(), &[Ok(message)]);
    }

    #[rstest]
    #[case(b" ?leading-space\n")]
    #[case(b"no-message-type\n")]
    #[case(b"?0\n")]
    #[case(b"?A_\n")]
    #[case(b"?A[\n")]
    #[case(b"?a[1\n")]
    #[case(b"?a[0]\n")]
    #[case(b"?a[-1]\n")]
    #[case(b"?a[2147483648]\n")]
    #[case(b"?a[10000000000]\n")]
    #[case(b"?a[1]x\n")]
    #[case(b"?a \0\n")]
    #[case(b"?a \x1B\n")]
    #[case(b"?a \\\n")]
    #[case(b"?a \\z\n")]
    fn test_fail(#[case] input: &[u8], mut parser: Parser) {
        let messages: Vec<_> = parser.append(input).collect();
        assert!(matches!(messages.as_slice(), &[Err(_)]));
    }

    #[test]
    fn test_too_long() {
        let mut parser = Parser::new(10);
        let messages: Vec<_> = parser.append(&b"?hello1234\n").collect();
        assert_eq!(
            messages.as_slice(),
            &[Err(ParseError::new("Line too long", 11))]
        );
        let messages: Vec<_> = parser.append(&b"?hello123\n").collect();
        assert_eq!(messages.as_slice(), &[Ok(msg!(Request, b"hello123", None))]);
    }

    fn split_points_strategy(size: usize) -> impl Strategy<Value = Vec<usize>> {
        prop::collection::vec(1..(size - 1), 1..10).prop_map(move |mut x| {
            x.push(0);
            x.push(size);
            x.sort();
            x
        })
    }

    /// Strategy that produces a message and some points at which to cut it.
    fn split_message_strategy() -> impl Strategy<Value = (String, Vec<usize>)> {
        text_message_strategy().prop_flat_map(|x| {
            let len = x.as_bytes().len();
            (Just(x), split_points_strategy(len))
        })
    }

    proptest! {
        /// Test that a variety of valid messages parse successfully
        #[test]
        fn success(input in text_message_strategy()) {
            let mut parser = Parser::new(usize::MAX);
            let messages: Vec<_> = parser.append(input.as_bytes()).collect();
            assert!(matches!(messages.as_slice(), &[Ok(_)]));
        }

        /// Test that splitting a message doesn't change how it is parsed
        #[test]
        fn parse_split(input in split_message_strategy(), max_line_length in 1..1000usize) {
            let (data, splits) = &input;
            let data = data.as_bytes();
            let mut parser1 = Parser::new(max_line_length);
            let messages1: Vec<_> = parser1.append(data).collect();

            let mut parser2 = Parser::new(max_line_length);
            let mut messages2 = Vec::new();
            for i in 1..splits.len() {
                messages2.extend(parser2.append(&data[splits[i - 1]..splits[i]]));
            }

            assert_eq!(messages1, messages2);
        }
    }
}
