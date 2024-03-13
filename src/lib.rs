use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};
use std::borrow::Cow;
use thiserror::Error;

#[pyclass]
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum MessageType {
    Request,
    Reply,
    Inform,
}

#[pyclass(module = "katcp_codec")]
pub struct Message {
    #[pyo3(get)]
    pub message_type: MessageType,
    pub name: Vec<u8>,
    #[pyo3(get)]
    pub id: Option<i32>,
    pub arguments: Vec<Vec<u8>>,
}

impl Message {
    pub fn new(
        message_type: MessageType,
        name: Vec<u8>,
        id: Option<i32>,
        arguments: Vec<Vec<u8>>,
    ) -> Self {
        Self {
            message_type,
            name,
            id,
            arguments,
        }
    }
}

#[pymethods]
impl Message {
    #[getter]
    fn get_name(&self) -> Cow<'_, [u8]> {
        Cow::from(&self.name)
    }

    #[getter]
    fn get_arguments(&self) -> Vec<Cow<'_, [u8]>> {
        self.arguments.iter().map(Cow::from).collect()
    }
}

fn is_eol(ch: u8) -> bool {
    ch == b'\n' || ch == b'\r'
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum State {
    Start,          // Initial state
    Empty,          // Seen whitespace, so this can only legally be a blank line
    BeforeName,     // Seen the type, haven't started the name
    Name,           // Middle of the name
    BeforeId,       // After [ in message ID
    Id,             // Middle of the message ID
    AfterId,        // After the ] terminating the message ID
    BeforeArgument, // Seen some whitespace, haven't started the next argument yet
    Argument,       // Middle of an argument, not following a backslash
    ArgumentEscape, // Seen a backslash in an argument
    Error,          // Invalid character seen, waiting for the end-of-line
}

#[derive(Error, Debug)]
#[error("{message:?} at character {position:?}")]
pub struct ParseError {
    message: String,
    position: usize,
}

impl ParseError {
    fn new(message: String, position: usize) -> Self {
        Self { message, position }
    }
}

#[pyclass]
pub struct Parser {
    state: State,
    line_length: usize,
    max_line_length: usize,
    message_type: Option<MessageType>,
    name: Vec<u8>,
    id: Option<i32>,
    arguments: Vec<Vec<u8>>,
    error: Option<ParseError>,
}

impl Parser {
    pub fn new(max_line_length: usize) -> Self {
        Self {
            state: State::Start,
            line_length: 0,
            max_line_length,
            message_type: None,
            name: vec![],
            id: None,
            arguments: vec![],
            error: None,
        }
    }

    fn error(&mut self, message: impl Into<String>) {
        self.state = State::Error;
        self.arguments.clear(); // Free up memory
        self.error = Some(ParseError::new(message.into(), self.line_length + 1));
    }

    fn reset(&mut self) {
        self.state = State::Start;
        self.line_length = 0;
        self.message_type = None;
        self.name.clear();
        self.id = None;
        self.arguments.clear();
        self.error = None;
    }

    /// Append a character (must not be end-of-line)
    fn add_no_eol(&mut self, ch: u8) {
        assert!(!is_eol(ch));
        self.line_length += 1;
        if self.state != State::Error && self.line_length == self.max_line_length {
            self.error("Line too long");
            return;
        }
        match self.state {
            State::Start => {
                self.message_type = Some(match ch {
                    b' ' | b'\t' => {
                        self.state = State::Empty;
                        return;
                    }
                    b'?' => MessageType::Request,
                    b'!' => MessageType::Reply,
                    b'#' => MessageType::Inform,
                    _ => {
                        self.error("Invalid message type");
                        return;
                    }
                });
                self.state = State::BeforeName;
            }
            State::BeforeName => match ch {
                b'A'..=b'Z' | b'a'..=b'z' => {
                    self.name.push(ch);
                    self.state = State::Name;
                }
                _ => {
                    self.error("Message name started with invalid character");
                }
            },
            State::Name => match ch {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' => {
                    self.name.push(ch);
                }
                b' ' | b'\t' => {
                    self.state = State::BeforeArgument;
                }
                b'[' => {
                    self.state = State::BeforeId;
                }
                _ => {
                    self.error("Message name contains an invalid character");
                }
            },
            State::BeforeId => match ch {
                b'1'..=b'9' => {
                    self.id = Some((ch - b'0') as i32);
                    self.state = State::Id;
                }
                _ => {
                    self.error("Invalid character in message ID");
                }
            },
            State::Id => {
                let old_id = self.id.unwrap(); // guaranteed to be non-None by state machine
                match ch {
                    b'0'..=b'9' => {
                        let digit = (ch - b'0') as i32;
                        let new_id = old_id.checked_mul(10).and_then(|x| x.checked_add(digit));
                        if new_id.is_none() {
                            self.error("Message ID overflows");
                        } else {
                            self.id = new_id;
                        }
                    }
                    b']' => {
                        self.state = State::AfterId;
                    }
                    _ => {
                        self.error("Invalid character in message ID");
                    }
                }
            }
            State::AfterId => match ch {
                b' ' | b'\t' => {
                    self.state = State::BeforeArgument;
                }
                _ => {
                    self.error("No whitespace after message ID");
                }
            },
            State::BeforeArgument | State::Argument => {
                if ch == b' ' || ch == b'\t' {
                    self.state = State::BeforeArgument;
                } else {
                    // If we're moving from BeforeArgument to Argument, create
                    // the slot for the argument
                    if self.state == State::BeforeArgument {
                        self.arguments.push(vec![]);
                    }
                    self.state = State::Argument;
                    match ch {
                        b'\\' => {
                            self.state = State::ArgumentEscape;
                        }
                        b'\0' | b'\x1B' => {
                            self.error("Invalid character");
                        }
                        _ => {
                            self.arguments.last_mut().unwrap().push(ch);
                        }
                    }
                }
            }
            State::ArgumentEscape => {
                if ch != b'@' {
                    // \@ is a special case: it's an empty string rather than a char
                    let escaped = match ch {
                        b'\\' => b'\\',
                        b'_' => b' ',
                        b'0' => b'\0',
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b'e' => b'\x1B',
                        b't' => b'\t',
                        _ => {
                            self.error("Invalid escape sequence");
                            return;
                        }
                    };
                    self.arguments.last_mut().unwrap().push(escaped);
                }
                self.state = State::Argument;
            }
            State::Empty => match ch {
                b' ' | b'\t' => {}
                _ => {
                    self.error("Line started with whitespace but contains non-whitespace");
                }
            },
            State::Error => {
                // Don't need to do anything in error state
            }
        }
    }

    /// Complete parsing a line and return the message
    fn add_eol(&mut self) -> Result<Option<Message>, ParseError> {
        match self.state {
            State::Start | State::Empty => {
                self.reset();
                return Ok(None);
            }
            State::BeforeName => {
                self.error("End of line before message name");
            }
            State::BeforeId | State::Id => {
                self.error("End of line in message ID");
            }
            State::ArgumentEscape => {
                self.error("End of line in middle of escape sequence");
            }
            State::Name | State::AfterId | State::BeforeArgument | State::Argument => {
                // All of these are valid terminal states
                let msg = Message::new(
                    self.message_type.take().unwrap(),
                    std::mem::take(&mut self.name),
                    self.id,
                    std::mem::take(&mut self.arguments),
                );
                self.reset();
                return Ok(Some(msg));
            }
            State::Error => {}
        }
        // If we get here, we had an error
        assert!(self.state == State::Error);
        let error = self.error.take().unwrap();
        self.reset();
        Err(error)
    }

    pub fn add(&mut self, ch: u8) -> Result<Option<Message>, ParseError> {
        if is_eol(ch) {
            self.add_eol()
        } else {
            self.add_no_eol(ch);
            Ok(None)
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
    fn append<'py>(
        &mut self,
        py: Python<'py>,
        data: Bound<'py, PyBytes>,
    ) -> PyResult<Bound<'py, PyList>> {
        let out = PyList::empty_bound(py);
        for ch in data.as_bytes().iter() {
            match self.add(*ch) {
                Ok(Some(msg)) => {
                    out.append(Bound::new(py, msg)?)?;
                }
                Ok(None) => {}
                Err(error) => {
                    out.append(PyValueError::new_err(error.to_string()).into_value(py))?;
                }
            }
        }
        Ok(out)
    }
}

#[pymodule]
fn _lib(m: Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<MessageType>()?;
    m.add_class::<Message>()?;
    m.add_class::<Parser>()?;
    Ok(())
}
