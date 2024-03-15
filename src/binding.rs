use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};

use crate::message::{Message, MessageType};
use crate::parse::Parser;

#[pyclass(name = "Message", module = "katcp_codec._lib", get_all)]
pub struct PyMessage {
    pub message_type: MessageType,
    pub name: Py<PyBytes>,
    pub id: Option<i32>,
    pub arguments: Py<PyList>,
}

impl PyMessage {
    pub fn new(
        message_type: MessageType,
        name: Py<PyBytes>,
        id: Option<i32>,
        arguments: Py<PyList>,
    ) -> Self {
        Self {
            message_type,
            name,
            id,
            arguments,
        }
    }
}

impl<'data> ToPyObject for Message<'data> {
    fn to_object(&self, py: Python<'_>) -> PyObject {
        let py_msg = PyMessage::new(
            self.message_type,
            PyBytes::new_bound(py, &self.name).unbind(),
            self.id,
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
