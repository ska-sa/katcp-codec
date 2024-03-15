use pyo3::prelude::*;

use crate::message::{Message, MessageType};
use crate::parse::Parser;

#[pymodule]
fn _lib(m: Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<MessageType>()?;
    m.add_class::<Message>()?;
    m.add_class::<Parser>()?;
    Ok(())
}
