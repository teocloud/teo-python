use indexmap::IndexMap;
use pyo3::{import_exception, types::{PyDict, PyType}, IntoPy, PyErr, PyObject, PyResult, Python};

import_exception!(teo, TeoException);

pub trait IntoTeoResult<T> {
    fn into_teo_result(self) -> ::teo::prelude::Result<T>;
}

impl<T> IntoTeoResult<T> for PyResult<T> {
    fn into_teo_result(self) -> teo::prelude::Result<T> {
        Python::with_gil(|py| {
            match self {
                Ok(r) => Ok(r),
                Err(e) => {
                    if e.get_type(py).is(PyType::new::<TeoException>(py)) {
                        let py_object: PyObject = e.clone_ref(py).into_py(py);
                        let message: String = py_object.getattr(py, "message").into_teo_result()?.extract(py).into_teo_result()?;
                        let code: Option<u16> = py_object.getattr(py, "code").into_teo_result()?.extract(py).into_teo_result()?;
                        let title: Option<String> = py_object.getattr(py, "title").into_teo_result()?.extract(py).into_teo_result()?;
                        let prefixes: Option<Vec<String>> = py_object.getattr(py, "prefixes").into_teo_result()?.extract(py).into_teo_result()?;
                        let errors_py = py_object.getattr(py, "errors").into_teo_result()?;
                        let errors = if errors_py.is_none(py) {
                            None
                        } else {
                            let dict: &PyDict = errors_py.extract(py).into_teo_result()?;
                            let mut map_result: IndexMap<String, String> = IndexMap::new();
                            for (k, v) in dict.iter() {
                                let k_string: String = k.extract().into_teo_result()?;
                                let v_string: String = v.extract().into_teo_result()?;
                                map_result.insert(k_string, v_string);
                            }
                            Some(map_result)
                        };
                        let mut error = ::teo::prelude::Error::new(message);
                        error.code = code;
                        error.title = title;
                        error.prefixes = prefixes;
                        error.errors = errors;
                        error.assign_platform_native_object(e);
                        Err(error)
                    } else {
                        let mut error = ::teo::prelude::Error::new(e.to_string());
                        error.assign_platform_native_object(e);
                        Err(error)
                    }
                },
            }    
        })
    }
}

pub trait IntoPyResult<T> {
    fn into_py_result(self, py: Python<'_>) -> PyResult<T>;
}

impl<T> IntoPyResult<T> for teo::prelude::Result<T> {
    fn into_py_result(self, py: Python<'_>) -> PyResult<T> {
        match self {
            Ok(r) => Ok(r),
            Err(e) => {
                let meta: Option<&PyErr> = e.platform_native_object();
                if let Some(err) = meta {
                    Err(PyErr::from_value(err.into_py(py).as_ref(py)))
                } else {
                    let err = TeoException::new_err("");
                    let py_object: PyObject = err.clone_ref(py).into_py(py);
                    py_object.setattr(py, "message", e.message())?;
                    py_object.setattr(py, "title", e.title.clone())?;
                    py_object.setattr(py, "code", e.code)?;
                    if let Some(errors) = e.errors {
                        let dict = PyDict::new(py);
                        for (k, v) in errors {
                            dict.set_item(k, v)?;
                        }
                        py_object.setattr(py, "errors", dict)?;
                    } else {
                        py_object.setattr(py, "errors", ())?;
                    }
                    py_object.setattr(py, "prefixes", e.prefixes.clone())?;
                    Err(err)
                }
            },
        }
    }
}

pub trait IntoPyResultWithGil<T> {
    fn into_py_result_with_gil(self) -> PyResult<T>;
}

impl<T> IntoPyResultWithGil<T> for teo::prelude::Result<T> {
    fn into_py_result_with_gil(self) -> PyResult<T> {
        Python::with_gil(|py| {
            self.into_py_result(py)
        })
    }
}
