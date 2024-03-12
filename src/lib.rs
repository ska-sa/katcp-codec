use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};

fn is_eol(ch: u8) -> bool {
    ch == b'\n' || ch == b'\r'
}

fn is_eol_terminated(line: &[u8]) -> bool {
    match line.last() {
        Some(x) if is_eol(*x) => true,
        _ => false
    }
}

#[pyclass]
#[derive(Clone, Eq, PartialEq, Debug)]
enum MessageType {
    Request,
    Reply,
    Inform,
}

#[pyclass]
struct InvalidMessage {
    error: String,
}

impl InvalidMessage {
    fn new(error: impl Into<String>) -> Self {
        Self { error: error.into() }
    }
}

#[pyclass(module="katcp_codec", get_all)]
struct Message {
    pub message_type: MessageType,
    pub id: Option<u32>,
    pub arguments: Vec<Vec<u8>>,
}

enum ParseResult {
    Message(Message),
    Error(InvalidMessage),
    Empty,
}

impl Message {
    fn parse(_line: &[u8]) -> ParseResult {
        // TODO: implement. Also figure out how to handle whitespace-only lines, and errors
        let msg = Self { message_type: MessageType::Request, id: None, arguments: vec![] };
        ParseResult::Message(msg)
    }
}

#[pyclass]
struct Parser {
    max_line_length: usize,
    current: Vec<u8>,
    overflow: bool,
}

#[pymethods]
impl Parser {
    #[new]
    fn py_new(max_line_length: usize) -> Self {
        Self { max_line_length, current: vec![], overflow: false }
    }

    // TODO: support buffer protocol?
    fn append<'py>(&mut self, py: Python<'py>, data: Bound<'py, PyBytes>) -> PyResult<Bound<'py, PyList>> {
        let out = PyList::empty_bound(py);
        for piece in data.as_bytes().split_inclusive(|x| is_eol(*x)) {
            if !self.overflow && piece.len() <= self.max_line_length - self.current.len() {
                self.current.extend_from_slice(piece);
                if is_eol_terminated(&self.current) {
                    match Message::parse(&self.current) {
                        ParseResult::Message(msg) => {
                            out.append(Bound::new(py, msg)?)?;
                        }
                        ParseResult::Error(msg) => {
                            out.append(Bound::new(py, msg)?)?;
                        }
                        ParseResult::Empty => {}
                    }
                    self.current.clear();
                }
            } else {
                self.overflow = true;
                self.current.clear();
                if is_eol_terminated(&piece) {
                    out.append(Bound::new(py, InvalidMessage::new("Line too long"))?)?;
                    self.overflow = false;
                }
            }
        }
        Ok(out)
    }
}

#[pymodule]
fn _lib(m: Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Message>()?;
    m.add_class::<Parser>()?;
    Ok(())
}
