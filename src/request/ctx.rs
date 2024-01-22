use pyo3::{pyclass, pymethods, PyObject, PyResult, Python};
use teo::prelude::request::Ctx as TeoRequestCtx;
use crate::{object::value::teo_value_to_py_any, dynamic::py_ctx_object_from_teo_transaction_ctx};

use super::{Request, HandlerMatch};

#[pyclass]
pub struct RequestCtx {
    teo_inner: TeoRequestCtx,
}

/// HTTP request.
#[pymethods]
impl RequestCtx {

    pub fn request(&self) -> Request {
        Request {
            teo_request: self.teo_inner.request().clone()
        }
    }

    pub fn body(&self, py: Python<'_>) -> PyResult<PyObject> {
        teo_value_to_py_any(py, self.teo_inner.body())
    }

    pub fn teo(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(py_ctx_object_from_teo_transaction_ctx(py, self.teo_inner.transaction_ctx(), "")?)
    }

    pub fn handler_match(&'static self) -> HandlerMatch {
        HandlerMatch {
            teo_inner: self.teo_inner.handler_match()
        }
    }
}
