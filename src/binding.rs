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

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};

use crate::message::{Message, MessageType};
use crate::parse::Parser;

#[pyclass(name = "Message", module = "katcp_codec._lib", get_all)]
pub struct PyMessage {
    pub mtype: MessageType,
    pub name: Py<PyBytes>,
    pub mid: Option<i32>,
    pub arguments: Py<PyList>,
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
            name,
            mid,
            arguments,
        }
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

#[pymodule]
fn _lib(m: Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<MessageType>()?;
    m.add_class::<PyMessage>()?;
    m.add_class::<Parser>()?;
    Ok(())
}
