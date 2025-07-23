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

use adjacent_pair_iterator::AdjacentPairIterator;
use pyo3::buffer::{Element, PyBuffer, ReadOnlyCell};
use pyo3::exceptions::{PyBufferError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyList;
use thiserror::Error;

use katcp_codec_fsm::{Action, MessageType, State};

use crate::tables::PARSER_TABLE;

/// A katcp message produced by parsing.
///
/// To minimise the number of memory allocations, the name and the arguments
/// are all stored back-to-back in a [Vec<u8>], and the individual fields just
/// store offsets into this vector. This allows a message with many arguments
/// to use only O(1) allocations.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct Message {
    pub mtype: MessageType,
    /// Message ID, if present. It must be positive.
    pub mid: Option<u32>,
    /// Starting position of each argument in [storage]. A final element
    /// is added to indicate the end of the last argument.
    argument_start: Vec<usize>,
    storage: Vec<u8>,
}

impl Message {
    /// Get the name of the message
    pub fn name(&self) -> &[u8] {
        &self.storage[..self.argument_start[0]]
    }

    /// Iterate over the arguments of the message
    pub fn arguments(&self) -> impl ExactSizeIterator<Item = &[u8]> {
        self.argument_start
            .iter()
            .adjacent_pairs()
            .map(|(&a, &b)| &self.storage[a..b])
    }

    /// Construct a message. This is inefficient and only used for tests.
    #[cfg(test)]
    pub fn new<N, I, A>(mtype: MessageType, name: N, mid: Option<u32>, arguments: I) -> Self
    where
        N: AsRef<[u8]>,
        I: IntoIterator<Item = A>,
        A: AsRef<[u8]>,
    {
        let name = name.as_ref();
        let mut msg = Message {
            mtype,
            mid,
            argument_start: vec![name.len()],
            storage: name.to_vec(),
        };
        for argument in arguments.into_iter() {
            msg.storage.extend_from_slice(argument.as_ref());
            msg.argument_start.push(msg.storage.len());
        }
        msg
    }
}

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

/// Abstract read access to either [T] or [ReadOnlyCell<T>].
pub trait ReadAccess<T: Copy>: Sized {
    fn read(&self) -> T;
}

impl<T: Copy> ReadAccess<T> for T {
    #[inline]
    fn read(&self) -> T {
        *self
    }
}

impl<T: Element> ReadAccess<T> for ReadOnlyCell<T> {
    #[inline]
    fn read(&self) -> T {
        self.get()
    }
}

/// Iterator implementation for [Parser::append].
pub struct ParseIterator<'parser, 'data, T>
where
    'data: 'parser,
    T: ReadAccess<u8>,
{
    parser: &'parser mut Parser,
    data: &'data [T],
}

impl<'parser, 'data, T> Iterator for ParseIterator<'parser, 'data, T>
where
    'data: 'parser,
    T: ReadAccess<u8>,
{
    type Item = Result<Message, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        let (msg, tail) = self.parser.next_message(self.data);
        self.data = tail;
        msg
    }
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
    /// Message ID, or [None] if there isn't one or we haven't parsed one yet
    mid: Option<u32>,
    /// Positions that arguments start in [Parser::storage]. This is empty if
    /// we haven't yet gone past state Name.
    argument_start: Vec<usize>,
    /// Storage for name and arguments
    storage: Vec<u8>,
    /// Current error, if we are in an error state
    error: Option<ParseError>,
}

impl Parser {
    /// Create a new parser.
    pub fn new(max_line_length: usize) -> Self {
        Self {
            state: State::Start,
            line_length: 0,
            max_line_length,
            mtype: None,
            mid: None,
            argument_start: vec![],
            storage: vec![],
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
        self.mid = None;
        self.argument_start.clear();
        self.storage.clear();
        self.error = None;
    }

    /// Signal an error at a particular position on a line.
    fn error_at(&mut self, message: impl Into<String>, position: usize) {
        if self.state != State::ErrorEndOfLine {
            self.state = State::Error;
        }
        if self.error.is_none() {
            self.error = Some(ParseError::new(message.into(), position));
        }
        // Free up some memory early
        self.argument_start.clear();
        self.storage.clear();
    }

    /// Signal an error at the current position.
    fn error(&mut self, message: impl Into<String>) {
        self.error_at(message, self.line_length + 1);
    }

    /// Apply an [Action] to the parser.
    ///
    /// `position` is the number of characters that appeared on the line
    /// prior to `chunk`. On the other hand, [Parser::line_length] includes
    /// the length of the chunk.
    fn apply<T: ReadAccess<u8>>(
        &mut self,
        action: &Action,
        chunk: &[T],
        position: usize,
    ) -> Result<Option<Message>, ParseError> {
        match action {
            Action::SetType(mtype) => {
                self.mtype = Some(*mtype);
            }
            Action::Name => {
                self.storage.extend(chunk.iter().map(T::read));
            }
            Action::Id => {
                // TODO: optimise this using the whole chunk at once
                for ch in chunk.iter() {
                    // Compute the update in 64-bit to detect overflow at the end
                    let mid = self.mid.unwrap_or(0) as u64;
                    let mid = mid * 10 + ((ch.read() - b'0') as u64);
                    if let Ok(value) = i32::try_from(mid) {
                        self.mid = Some(value as u32);
                    } else {
                        self.error_at("Message ID overflowed", position);
                        break;
                    }
                }
            }
            Action::Argument => {
                self.storage.extend(chunk.iter().map(T::read));
            }
            Action::ArgumentEscaped(c) => {
                self.storage.push(*c);
            }
            Action::ResetLineLength => {
                self.line_length = 0;
            }
            Action::Nothing => {}
            Action::Error => {
                self.error_at("Invalid character", position);
            }
        }

        match self.state {
            State::EndOfLine => {
                // Indicate end of last argument (or of name, if no arguments)
                self.argument_start.push(self.storage.len());
                let msg = Message {
                    mtype: self.mtype.take().unwrap(),
                    mid: self.mid,
                    argument_start: std::mem::take(&mut self.argument_start),
                    storage: std::mem::take(&mut self.storage),
                };
                self.reset();
                Ok(Some(msg))
            }
            State::ErrorEndOfLine => {
                let error = self.error.take().unwrap();
                self.reset();
                Err(error)
            }
            _ => Ok(None),
        }
    }

    /// Consume data until new end-of-line is seen, returning the message if any.
    fn next_message<'data, T: ReadAccess<u8>>(
        &mut self,
        mut data: &'data [T],
    ) -> (Option<Result<Message, ParseError>>, &'data [T]) {
        while !data.is_empty() {
            if self.line_length >= self.max_line_length && self.state != State::Error {
                self.error("Line too long");
            }

            let entry = &PARSER_TABLE[self.state][data[0].read()];
            if entry.create_argument {
                self.argument_start.push(self.storage.len());
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
                while p < max_len && fast_table[data[p].read()] {
                    p += 1;
                }
            }

            let position = self.line_length + 1;
            if self.line_length < self.max_line_length {
                // The max_len calculation guarantees that this won't exceed
                // max_line_length.
                self.line_length += p;
            }

            let result = self.apply(&entry.action, &data[..p], position);
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
        (None, data)
    }

    /// Add data to the parser and return an iterator over messages that arise.
    ///
    /// The data is only consumed as a result of iteration. Dropping the
    /// iterator without fully consuming it has undefined results.
    #[must_use = "Must consume the returned iterator for anything to happen"]
    pub fn append<'parser, 'data, D, T>(
        &'parser mut self,
        data: &'data D,
    ) -> ParseIterator<'parser, 'data, T>
    where
        D: AsRef<[T]> + ?Sized,
        T: ReadAccess<u8>,
    {
        ParseIterator {
            parser: self,
            data: data.as_ref(),
        }
    }
}

#[pymethods]
impl Parser {
    #[new]
    fn py_new(max_line_length: usize) -> Self {
        Self::new(max_line_length)
    }

    #[pyo3(name = "append")]
    fn py_append<'py>(
        &mut self,
        py: Python<'py>,
        buf: PyBuffer<u8>,
    ) -> PyResult<Bound<'py, PyList>> {
        let slice = buf
            .as_slice(py)
            .ok_or_else(|| PyBufferError::new_err("Buffer object is not C-contiguous"))?;
        let out = PyList::empty(py);
        for result in self.append(slice) {
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
            $crate::parse::Message::new(
                $mtype,
                $name,
                $mid,
                std::vec::Vec::<&[u8]>::new(),
            )
        };
        ( $mtype:expr, $name:literal, $mid:expr, $($arg:literal),* $(,)? ) => {
            $crate::parse::Message::new(
                $mtype,
                $name.as_slice(),
                $mid,
                std::vec![$( $arg.as_slice() ),*],
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
    fn test_simple(#[case] input: &[u8], #[case] message: Message, mut parser: Parser) {
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
