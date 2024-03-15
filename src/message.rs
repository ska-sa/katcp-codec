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

pub struct Message<'data> {
    pub message_type: MessageType,
    pub name: Cow<'data, [u8]>,
    pub id: Option<i32>,
    pub arguments: Vec<Cow<'data, [u8]>>,
}

impl<'data> Message<'data> {
    pub fn new(
        message_type: MessageType,
        name: Cow<'data, [u8]>,
        id: Option<i32>,
        arguments: Vec<Cow<'data, [u8]>>,
    ) -> Self {
        Self {
            message_type,
            name,
            id,
            arguments,
        }
    }
}
