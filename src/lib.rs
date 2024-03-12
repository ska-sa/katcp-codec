use pyo3::prelude::*;

#[pyfunction]
fn double(x: i32) -> i32 {
    x * 2
}

#[pymodule]
fn _lib(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(double, m)?)?;
    Ok(())
}
