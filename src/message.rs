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

//! The basic katcp message type

use pyo3::exceptions::PyRuntimeError;
use pyo3::gc::PyVisit;
use pyo3::prelude::*;
use pyo3::pybacked::PyBackedBytes;
use pyo3::types::{PyBytes, PyList};
use pyo3::PyTraverseError;
use std::borrow::Cow;
use uninit::prelude::*;

pub use katcp_codec_fsm::MessageType;

/// A katcp message. The name and arguments can either own their data or
/// reference existing data from a buffer.
///
/// The katcp specification is byte-oriented, so the text fields are \[u8\]
/// rather than [str]. The name has a restricted character set that ensures
/// it can be decoded as ASCII (or UTF-8) but the arguments may contain
/// arbitrary bytes.
///
/// The message ID and name are *not* validated when constructed with
/// [Message::new]. Using an invalid value for either will not panic, but
/// will lead to invalid formatting from [Message::write_out].
#[derive(Clone, Eq, Debug)]
pub struct Message<N, A>
where
    N: AsRef<[u8]>,
    A: AsRef<[u8]>,
{
    /// Message type
    pub mtype: MessageType,
    /// Message name
    pub name: N,
    /// Message ID, if present. It must be positive.
    pub mid: Option<u32>,
    /// Message arguments
    pub arguments: Vec<A>,
}

impl<N, A, N2, A2> PartialEq<Message<N2, A2>> for Message<N, A>
where
    N: AsRef<[u8]> + PartialEq<N2>,
    A: AsRef<[u8]> + PartialEq<A2>,
    N2: AsRef<[u8]>,
    A2: AsRef<[u8]>,
{
    fn eq(&self, other: &Message<N2, A2>) -> bool {
        self.mtype == other.mtype
            && self.name == other.name
            && self.mid == other.mid
            && self.arguments == other.arguments
    }
}

impl<N, A> Message<N, A>
where
    N: AsRef<[u8]>,
    A: AsRef<[u8]>,
{
    /// Create a new message.
    pub fn new(
        mtype: MessageType,
        name: impl Into<N>,
        mid: Option<u32>,
        arguments: impl Into<Vec<A>>,
    ) -> Self {
        Self {
            mtype,
            name: name.into(),
            mid,
            arguments: arguments.into(),
        }
    }
}

/// Message type used for interaction with Python.
#[pyclass(name = "Message", module = "katcp_codec._lib", get_all, set_all)]
pub struct PyMessage {
    pub mtype: MessageType,
    pub name: Py<PyBytes>,
    pub mid: Option<u32>,
    pub arguments: Py<PyList>,
}

impl PyMessage {
    /// Construct a new message.
    pub fn new(
        mtype: MessageType,
        name: Py<PyBytes>,
        mid: Option<u32>,
        arguments: Py<PyList>,
    ) -> Self {
        Self {
            mtype,
            name,
            mid,
            arguments,
        }
    }
}

#[pymethods]
impl PyMessage {
    #[new]
    #[pyo3(signature = (mtype, name, mid, arguments))]
    fn py_new<'py>(
        mtype: MessageType,
        name: Bound<'py, PyBytes>,
        mid: Option<u32>,
        arguments: Bound<'py, PyList>,
    ) -> Self {
        Self::new(mtype, name.unbind(), mid, arguments.unbind())
    }

    // See https://pyo3.rs/v0.21.2/class/protocols#garbage-collector-integration
    fn __traverse__(&self, visit: PyVisit) -> Result<(), PyTraverseError> {
        visit.call(&self.name)?;
        visit.call(&self.arguments)?;
        Ok(())
    }

    fn __clear__(&mut self) {
        // It should be safe not to do anything here:
        // - `name` is a `bytes` which cannot hold any references
        // - `arguments` is a `list` and Python will clear all references held
        //   by the list.
    }

    fn __bytes__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let name = self.name.bind_borrowed(py);
        // TODO: this is creating a new vector to hold the arguments.
        // Can we use another trait to handle directly iterating the PyList?
        let arguments: Vec<PyBackedBytes> = self.arguments.extract(py)?;
        let message = Message {
            mtype: self.mtype,
            name: name.as_bytes(),
            mid: self.mid,
            arguments,
        };
        let size = message.write_size();
        PyBytes::new_with(py, size, |bytes: &mut [u8]| {
            let remain = message.write_out(bytes.as_out());
            if !remain.is_empty() {
                // This should be unreachable, because we hold the GIL.
                Err(PyRuntimeError::new_err(
                    "Message changed size during formatting",
                ))
            } else {
                Ok(())
            }
        })
    }
}

impl<'py, N, A> IntoPyObject<'py> for Message<N, A>
where
    N: AsRef<[u8]>,
    A: AsRef<[u8]>,
{
    type Target = PyMessage;
    type Output = Bound<'py, Self::Target>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> PyResult<Self::Output> {
        let py_msg = PyMessage::new(
            self.mtype,
            PyBytes::new(py, self.name.as_ref()).unbind(),
            self.mid,
            PyList::new(py, self.arguments.iter().map(|x| Cow::from(x.as_ref())))?.unbind(),
        );
        py_msg.into_pyobject(py)
    }
}
