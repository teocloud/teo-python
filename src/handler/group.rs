use pyo3::{pyclass, pymethods, Py, PyErr, PyObject, PyResult, Python};
use teo::prelude::{handler::Group as TeoHandlerGroup, request};

use crate::dynamic::py_ctx_object_from_teo_transaction_ctx;
use crate::object::value::teo_value_to_py_any;
use crate::request::request::Request;
use crate::response::Response;
use crate::utils::await_coroutine_if_needed::{await_coroutine_if_needed, await_coroutine_if_needed_async_value};
use crate::utils::check_callable::check_callable;
use crate::result::IntoTeoPathResult;

#[pyclass]
pub struct HandlerGroup {
    pub(crate) teo_handler_group: &'static mut TeoHandlerGroup,
}

#[pymethods]
impl HandlerGroup {

    pub fn define_handler(&mut self, py: Python<'_>, name: String, callback: PyObject) -> PyResult<()> {
        check_callable(callback.as_ref(py))?;
        let callback_owned = &*Box::leak(Box::new(Py::from(callback)));
        self.teo_handler_group.define_handler(name.as_str(), move |ctx: request::Ctx| async move {
            let result = Python::with_gil(|py| {
                let request = Request {
                    teo_request: ctx.request().clone()
                };
                let body = teo_value_to_py_any(py, &ctx.body())?;
                let py_ctx = py_ctx_object_from_teo_transaction_ctx(py, ctx.transaction_ctx(), "")?;
                let result = callback_owned.call1(py, (request, body, py_ctx))?;
                Ok::<PyObject, PyErr>(result)
            }).into_teo_path_result()?;
            let awaited_result = await_coroutine_if_needed_async_value(result).await.into_teo_path_result()?;
            Python::with_gil(|py| {
                let response: Response = awaited_result.extract(py).into_teo_path_result()?;
                Ok(response.teo_response.clone())
            })
        });
        Ok(())
    }
}