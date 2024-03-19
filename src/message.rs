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

use pyo3::gc::PyVisit;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};
use pyo3::PyTraverseError;

use std::borrow::Cow;

#[pyclass(module = "katcp_codec._lib")]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum MessageType {
    #[pyo3(name = "REQUEST")]
    Request = 1,
    #[pyo3(name = "REPLY")]
    Reply = 2,
    #[pyo3(name = "INFORM")]
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

#[pyclass(name = "Message", module = "katcp_codec._lib", get_all)]
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
