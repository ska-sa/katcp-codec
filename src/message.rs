use pyo3::prelude::*;

use std::borrow::Cow;

#[pyclass(module = "katcp_codec._lib")]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum MessageType {
    #[pyo3(name = "REQUEST")]
    Request,
    #[pyo3(name = "REPLY")]
    Reply,
    #[pyo3(name = "INFORM")]
    Inform,
}

#[pyclass(module = "katcp_codec._lib")]
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
