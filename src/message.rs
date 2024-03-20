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
use pyo3::gc::PyVisit;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};
use pyo3::PyTraverseError;

use std::borrow::Cow;

#[pyclass(module = "katcp_codec._lib", rename_all = "SCREAMING_SNAKE_CASE")]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum MessageType {
    Request = 1,
    Reply = 2,
    Inform = 3,
}

pub struct Message<'data> {
    pub mtype: MessageType,
    pub name: Cow<'data, [u8]>,
    pub mid: Option<i32>,
    pub arguments: Vec<Cow<'data, [u8]>>,
}

impl<'data> Message<'data> {
    pub fn new(
        mtype: MessageType,
        name: Cow<'data, [u8]>,
        mid: Option<i32>,
        arguments: Vec<Cow<'data, [u8]>>,
    ) -> Self {
        Self {
            mtype,
            name,
            mid,
            arguments,
        }
    }
}

#[pyclass(name = "Message", module = "katcp_codec._lib", get_all, set_all)]
pub struct PyMessage {
    pub mtype: MessageType,
    pub name: Option<Py<PyBytes>>, // Option only to support __clear__
    pub mid: Option<i32>,
    pub arguments: Option<Py<PyList>>, // Option only to support __clear__
}

impl PyMessage {
    pub fn new(
        mtype: MessageType,
        name: Py<PyBytes>,
        mid: Option<i32>,
        arguments: Py<PyList>,
    ) -> Self {
        Self {
            mtype,
            name: Some(name),
            mid,
            arguments: Some(arguments),
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
        mid: Option<i32>,
        arguments: Bound<'py, PyList>,
    ) -> Self {
        Self::new(mtype, name.unbind(), mid, arguments.unbind())
    }

    fn __traverse__(&self, visit: PyVisit) -> Result<(), PyTraverseError> {
        if let Some(name) = &self.name {
            visit.call(name)?;
        }
        if let Some(arguments) = &self.arguments {
            visit.call(arguments)?;
        }
        Ok(())
    }

    fn __clear__(&mut self) {
        self.name = None;
        self.arguments = None;
    }

    fn __bytes__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let name = self
            .name
            .as_ref()
            .ok_or_else(|| PyValueError::new_err("name is None"))?;
        let name = name.bind(py);
        let arguments = self
            .arguments
            .as_ref()
            .ok_or_else(|| PyValueError::new_err("name is None"))?;
        // TODO: this is creating a new vector to hold the arguments.
        // Can we use a trait to handle directly iterating the PyList?
        let arguments: Vec<Cow<'py, [u8]>> = arguments.bind(py).extract()?;
        let message = Message {
            mtype: self.mtype,
            name: Cow::from(name.as_bytes()),
            mid: self.mid,
            arguments,
        };
        PyBytes::new_bound_with(py, message.write_size(), |mut bytes: &mut [u8]| {
            message.write(&mut bytes).unwrap();
            Ok(())
        })
    }
}

impl<'data> ToPyObject for Message<'data> {
    fn to_object(&self, py: Python<'_>) -> PyObject {
        let py_msg = PyMessage::new(
            self.mtype,
            PyBytes::new_bound(py, &self.name).unbind(),
            self.mid,
            PyList::new_bound(py, self.arguments.iter()).unbind(),
        );
        py_msg.into_py(py)
    }
}
