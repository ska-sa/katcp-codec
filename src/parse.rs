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

use enum_map::{enum_map, Enum, EnumMap};
use once_cell::sync::OnceCell;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

use crate::message::{Message, MessageType};

type ParsedMessage<'data> = Message<Cow<'data, [u8]>, Cow<'data, [u8]>>;

/// State in the state machine
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash, Enum)]
enum State {
    /// Initial state
    Start,
    /// Seen whitespace, so this can only legally be a blank line
    Empty,
    /// Seen the type, haven't started the name
    BeforeName,
    /// Middle of the name
    Name,
    /// After [ in message ID
    BeforeId,
    /// Middle of the message ID
    Id,
    /// After the ] terminating the message ID
    AfterId,
    /// Seen some whitespace, haven't started the next argument yet
    BeforeArgument,
    /// Middle of an argument, not following a backslash
    Argument,
    /// Seen a backslash in an argument
    ArgumentEscape,
    /// Invalid character seen, waiting for the end-of-line
    #[default]
    Error,
    /// Terminal state for a valid line
    EndOfLine,
    /// Terminal state for an invalid line
    ErrorEndOfLine,
}

impl State {
    fn is_terminal(&self) -> bool {
        matches!(self, State::EndOfLine | State::ErrorEndOfLine)
    }
}

/// Transition action in the state machine
#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
enum Action {
    /// No action needed (e.g. skipping whitespace, or an error)
    #[default]
    Nothing,
    /// Append the current character to the name
    Name,
    /// Append a digit to the message ID
    Id,
    /// Append the current character to the argument
    Argument,
    /// Append a specific character to the argument
    ArgumentEscaped(u8),
    /// Set the message type
    SetType(MessageType),
    /// Set line_length back to 0 (after empty message)
    ResetLineLength,
    /// Set an error message
    Error,
}

impl Action {
    fn is_mergeable(&self) -> bool {
        matches!(
            self,
            Action::Nothing | Action::Name | Action::Id | Action::Argument | Action::Error
        )
    }
}

/// (state, char) entry in the state machine
#[derive(Clone, Default, Eq, PartialEq, Hash)]
struct Entry {
    /// Action to apply
    action: Action,
    /// Next state
    state: State,
    /// Whether to create a new argument before applying the action
    create_argument: bool,
    /// Following characters that can be merged into the action
    fast_table: Option<Arc<EnumMap<u8, bool>>>,
}

impl Entry {
    fn new_full(action: Action, state: State, create_argument: bool) -> Self {
        Self {
            action,
            state,
            create_argument,
            fast_table: None,
        }
    }

    fn new(action: Action, state: State) -> Self {
        Self::new_full(action, state, false)
    }

    fn error() -> Self {
        Self::new(Action::Error, State::Error)
    }
}

#[derive(Error, Clone, Debug, Eq, PartialEq)]
#[error("{message:?} at character {position:?}")]
pub struct ParseError {
    message: String,
    position: usize,
}

impl ParseError {
    fn new(message: impl Into<String>, position: usize) -> Self {
        Self {
            message: message.into(),
            position,
        }
    }
}

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
    /// Name which *replaces [Parser::name]
    name: Cow<'data, [u8]>,
    /// Arguments to *append* to [Parser::arguments]
    arguments: Vec<Cow<'data, [u8]>>,
}

#[pyclass(module = "katcp_codec._lib")]
pub struct Parser {
    state: State,
    line_length: usize,
    max_line_length: usize,
    mtype: Option<MessageType>,
    name: Vec<u8>,
    mid: Option<i32>,
    arguments: Vec<Vec<u8>>,
    error: Option<ParseError>,
    table: &'static EnumMap<State, EnumMap<u8, Entry>>,
}

/// Extend a Cow<'_, [T]> with new elements.
///
/// This is special-cased to borrow the elements if the Cow was empty.
fn extend_cow<'a>(cow: &mut Cow<'a, [u8]>, elements: &'a [u8]) {
    if cow.is_empty() {
        *cow = Cow::from(elements);
    } else {
        cow.to_mut().extend_from_slice(elements);
    }
}

impl Parser {
    fn make_table(callback: impl Fn(u8) -> Entry) -> EnumMap<u8, Entry> {
        let mut table = EnumMap::default();
        for ch in 0..=255u8 {
            table[ch] = callback(ch);
        }
        // Simplify the callers by applying some generic rules
        if table[b'\n'].state == State::Error {
            table[b'\n'].state = State::ErrorEndOfLine;
        }
        assert!(matches!(
            table[b'\n'].state,
            State::EndOfLine | State::ErrorEndOfLine | State::Start
        ));
        table[b'\t'] = table[b' '].clone();
        table[b'\r'] = table[b'\n'].clone();
        table
    }

    fn make_error() -> EnumMap<u8, Entry> {
        Self::make_table(|_| Entry::error())
    }

    fn make_start() -> EnumMap<u8, Entry> {
        Self::make_table(|ch| match ch {
            b' ' => Entry::new(Action::Nothing, State::Empty),
            b'?' => Entry::new(Action::SetType(MessageType::Request), State::BeforeName),
            b'!' => Entry::new(Action::SetType(MessageType::Reply), State::BeforeName),
            b'#' => Entry::new(Action::SetType(MessageType::Inform), State::BeforeName),
            b'\n' => Entry::new(Action::ResetLineLength, State::Start),
            _ => Entry::error(),
        })
    }

    fn make_empty() -> EnumMap<u8, Entry> {
        Self::make_table(|ch| match ch {
            b' ' => Entry::new(Action::Nothing, State::Empty),
            b'\n' => Entry::new(Action::ResetLineLength, State::Start),
            _ => Entry::error(),
        })
    }

    fn make_before_name() -> EnumMap<u8, Entry> {
        Self::make_table(|ch| match ch {
            b'A'..=b'Z' | b'a'..=b'z' => Entry::new(Action::Name, State::Name),
            _ => Entry::error(),
        })
    }

    fn make_name() -> EnumMap<u8, Entry> {
        Self::make_table(|ch| match ch {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' => Entry::new(Action::Name, State::Name),
            b' ' => Entry::new(Action::Nothing, State::BeforeArgument),
            b'[' => Entry::new(Action::Nothing, State::BeforeId),
            b'\n' => Entry::new(Action::Nothing, State::EndOfLine),
            _ => Entry::error(),
        })
    }

    fn make_before_id() -> EnumMap<u8, Entry> {
        Self::make_table(|ch| match ch {
            b'1'..=b'9' => Entry::new(Action::Id, State::Id),
            _ => Entry::error(),
        })
    }

    fn make_id() -> EnumMap<u8, Entry> {
        Self::make_table(|ch| match ch {
            b'0'..=b'9' => Entry::new(Action::Id, State::Id),
            b']' => Entry::new(Action::Nothing, State::AfterId),
            _ => Entry::error(),
        })
    }

    fn make_after_id() -> EnumMap<u8, Entry> {
        Self::make_table(|ch| match ch {
            b' ' => Entry::new(Action::Nothing, State::BeforeArgument),
            b'\n' => Entry::new(Action::Nothing, State::EndOfLine),
            _ => Entry::error(),
        })
    }

    /// Used for both State::BeforeArgument and State::Argument
    fn make_argument(create_argument: bool) -> EnumMap<u8, Entry> {
        Self::make_table(|ch| match ch {
            b' ' => Entry::new(Action::Nothing, State::BeforeArgument),
            b'\n' => Entry::new(Action::Nothing, State::EndOfLine),
            b'\\' => Entry::new_full(Action::Nothing, State::ArgumentEscape, create_argument),
            b'\0' | b'\x1B' => Entry::error(),
            _ => Entry::new_full(Action::Argument, State::Argument, create_argument),
        })
    }

    fn make_argument_escape() -> EnumMap<u8, Entry> {
        Self::make_table(|ch| match ch {
            b'@' => Entry::new(Action::Nothing, State::Argument),
            b'\\' => Entry::new(Action::ArgumentEscaped(b'\\'), State::Argument),
            b'_' => Entry::new(Action::ArgumentEscaped(b' '), State::Argument),
            b'0' => Entry::new(Action::ArgumentEscaped(b'\0'), State::Argument),
            b'n' => Entry::new(Action::ArgumentEscaped(b'\n'), State::Argument),
            b'r' => Entry::new(Action::ArgumentEscaped(b'\r'), State::Argument),
            b'e' => Entry::new(Action::ArgumentEscaped(b'\x1B'), State::Argument),
            b't' => Entry::new(Action::ArgumentEscaped(b'\t'), State::Argument),
            _ => Entry::error(),
        })
    }

    fn build_fast_tables(table: &mut EnumMap<State, EnumMap<u8, Entry>>) {
        type ActionDisc = std::mem::Discriminant<Action>;

        let mut cache: HashMap<(State, ActionDisc), Arc<EnumMap<u8, bool>>> = HashMap::new();

        // Rust borrowing rules complicate this looping. We need to mutate
        // the table, which we can't do if we're borrowing it for iteration.
        let states: Vec<State> = table
            .iter()
            .map(|(state, _)| state)
            .filter(|state| !state.is_terminal())
            .collect();
        for src_state in states {
            for ch in 0..=255u8 {
                let entry = &table[src_state][ch];
                if entry.state.is_terminal() || !entry.action.is_mergeable() {
                    continue;
                }
                let state = entry.state;
                let key = (state, std::mem::discriminant(&entry.action));
                // Lifetime of `entry` ends here, leaving `table` accessible

                let fast_table = cache.entry(key).or_insert_with(|| {
                    let mut result = EnumMap::default();
                    for ch2 in 0..=255u8 {
                        let entry = &table[state][ch2];
                        result[ch2] = entry.state == state
                            && std::mem::discriminant(&entry.action) == key.1
                            && !entry.create_argument;
                    }
                    Arc::new(result)
                });
                if fast_table.values().any(|x| *x) {
                    table[src_state][ch].fast_table = Some(fast_table.clone());
                }
            }
        }
    }

    fn parser_table() -> &'static EnumMap<State, EnumMap<u8, Entry>> {
        static INSTANCE: OnceCell<EnumMap<State, EnumMap<u8, Entry>>> = OnceCell::new();
        INSTANCE.get_or_init(|| {
            let mut table = enum_map! {
                State::Start => Self::make_start(),
                State::Empty => Self::make_empty(),
                State::BeforeName => Self::make_before_name(),
                State::Name => Self::make_name(),
                State::BeforeId => Self::make_before_id(),
                State::Id => Self::make_id(),
                State::AfterId => Self::make_after_id(),
                State::BeforeArgument => Self::make_argument(true),
                State::Argument => Self::make_argument(false),
                State::ArgumentEscape => Self::make_argument_escape(),
                State::Error => Self::make_error(),
                State::EndOfLine => Self::make_error(),
                State::ErrorEndOfLine => Self::make_error(),
            };
            Self::build_fast_tables(&mut table);
            table
        })
    }

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
            table: Self::parser_table(),
        }
    }

    /// Number of bytes currently buffered for an incomplete line.
    ///
    /// This is capped at [max_line_length], even if a longer (overflowing)
    /// line is in progress.
    pub fn buffer_size(&self) -> usize {
        self.line_length
    }

    /// Return the parser to its initial state
    pub fn reset(&mut self) {
        self.state = State::Start;
        self.line_length = 0;
        self.mtype = None;
        self.name.clear();
        self.mid = None;
        self.arguments.clear();
        self.error = None;
    }

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

    fn error(&mut self, transient: &mut Transient, message: impl Into<String>) {
        self.error_at(transient, message, self.line_length + 1);
    }

    fn reset_transient(&mut self, transient: &mut Transient<'_>) {
        self.reset();
        transient.name = Cow::default();
        transient.arguments.clear();
    }

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
                    let mid = self.mid.unwrap_or(0) as i64;
                    let mid = mid * 10 + ((*ch - b'0') as i64);
                    if let Ok(value) = i32::try_from(mid) {
                        self.mid = Some(value);
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

    /// Consume data until new end-of-line is seen, returning the remainder if any
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

            let entry = &self.table[self.state][data[0]];
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
        Parser::new(1000)
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
    fn test_simple(#[case] input: &[u8], #[case] message: ParsedMessage, mut parser: Parser) {
        let messages: Vec<_> = parser.append(input).collect();
        assert_eq!(messages.as_slice(), &[Ok(message)]);
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
