pub mod model_object_wrapper;
pub mod transaction_ctx_wrapper;
pub mod model_ctx_wrapper;

use std::collections::BTreeMap;
use indexmap::IndexMap;
use inflector::Inflector;
use pyo3::ffi::PyTypeObject;
use ::teo::prelude::App;
use pyo3::{IntoPy, PyAny, PyErr, PyObject, PyResult, Python};
use pyo3::exceptions::PyRuntimeError;
use pyo3::types::{PyCFunction, PyDict, PyList, PyNone};
use teo::prelude::{Namespace, Value, model, transaction};
use crate::dynamic::model_object_wrapper::ModelObjectWrapper;

use crate::object::value::{teo_value_to_py_any, py_any_to_teo_value};
use crate::result::{IntoPyResult, IntoPyResultWithGil};
use crate::utils::check_py_dict::check_py_dict;

use self::model_ctx_wrapper::ModelCtxWrapper;
use self::transaction_ctx_wrapper::TransactionCtxWrapper;

static mut CTXS: Option<&'static BTreeMap<String, PyObject>> = None;
static mut CLASSES: Option<&'static BTreeMap<String, PyObject>> = None;
static mut OBJECTS: Option<&'static BTreeMap<String, PyObject>> = None;

pub fn setup_dynamic_container() -> PyResult<()> {
    unsafe { CLASSES = Some(Box::leak(Box::new(BTreeMap::new()))) };
    unsafe { OBJECTS = Some(Box::leak(Box::new(BTreeMap::new()))) };
    unsafe { CTXS = Some(Box::leak(Box::new(BTreeMap::new()))) };
    Ok(())
}

fn classes_mut() -> &'static mut BTreeMap<String, PyObject> {
    unsafe {
        let const_ptr = CLASSES.unwrap() as *const BTreeMap<String, PyObject>;
        let mut_ptr = const_ptr as *mut BTreeMap<String, PyObject>;
        &mut *mut_ptr
    }
}

fn objects_mut() -> &'static mut BTreeMap<String, PyObject> {
    unsafe {
        let const_ptr = OBJECTS.unwrap() as *const BTreeMap<String, PyObject>;
        let mut_ptr = const_ptr as *mut BTreeMap<String, PyObject>;
        &mut *mut_ptr
    }
}

fn ctxs_mut() -> &'static mut BTreeMap<String, PyObject> {
    unsafe {
        let const_ptr = CTXS.unwrap() as *const BTreeMap<String, PyObject>;
        let mut_ptr = const_ptr as *mut BTreeMap<String, PyObject>;
        &mut *mut_ptr
    }
}

pub fn get_model_class_class(py: Python<'_>, name: &str) -> PyResult<PyObject> {
    unsafe {
        if let Some(object_ref) = CLASSES.unwrap().get(name) {
            Ok(object_ref.clone_ref(py))
        } else {
            generate_model_class_class(py, name)
        }
    }
}

pub fn get_model_object_class(py: Python<'_>, name: &str) -> PyResult<PyObject> {
    unsafe {
        if let Some(object_ref) = OBJECTS.unwrap().get(name) {
            Ok(object_ref.clone_ref(py))
        } else {
            generate_model_object_class(py, name)
        }
    }
}

pub fn get_ctx_class(py: Python<'_>, name: &str) -> PyResult<PyObject> {
    unsafe {
        if let Some(object_ref) = CTXS.unwrap().get(name) {
            Ok(object_ref.clone_ref(py))
        } else {
            generate_ctx_class(py, name)
        }
    }
}

pub(crate) fn py_model_class_object_from_teo_model_ctx(py: Python<'_>, model_ctx: model::Ctx, name: &str) -> PyResult<PyObject> {
    let model_name = model_ctx.model.path().join(".");
    let model_class_class = get_model_class_class(py, &model_name)?;
    let model_class_object = model_class_class.call_method1(py, "__new__", (model_class_class.as_ref(py),))?;
    model_class_object.setattr(py, "__teo_model_ctx__", ModelCtxWrapper::new(model_ctx))?;
    Ok(model_class_object)
}

pub(crate) fn py_model_object_from_teo_model_object(py: Python<'_>, teo_model_object: model::Object) -> PyResult<PyObject> {
    let model_name = teo_model_object.model().path().join(".");
    let model_object_class = get_model_object_class(py, &model_name)?;
    let model_object = model_object_class.call_method1(py, "__new__", (model_object_class.as_ref(py),))?;
    model_object.setattr(py, "__teo_object__", ModelObjectWrapper::new(teo_model_object))?;
    Ok(model_object)
}

pub(crate) fn py_optional_model_object_from_teo_object(py: Python<'_>, teo_model_object: Option<model::Object>) -> PyResult<PyObject> {
    Ok(match teo_model_object {
        Some(teo_model_object) => py_model_object_from_teo_model_object(py, teo_model_object)?,
        None => ().into_py(py),
    })
}

pub(crate) fn py_ctx_object_from_teo_transaction_ctx(py: Python<'_>, transaction_ctx: transaction::Ctx, name: &str) -> PyResult<PyObject> {
    let ctx_class = get_ctx_class(py, name)?;
    let ctx_object = ctx_class.call_method1(py, "__new__", (ctx_class.as_ref(py),))?;
    ctx_object.setattr(py, "__teo_transaction_ctx__", TransactionCtxWrapper::new(transaction_ctx))?;
    Ok(ctx_object)
}

pub(crate) fn teo_model_ctx_from_py_model_class_object(py: Python<'_>, model_class_object: PyObject) -> PyResult<model::Ctx> {
    let wrapper: ModelCtxWrapper = model_class_object.getattr(py, "__teo_model_ctx__")?.extract(py)?;
    Ok(wrapper.ctx.clone())
}

pub(crate) fn teo_model_object_from_py_model_object(py: Python<'_>, model_class_object: PyObject) -> PyResult<model::Object> {
    let wrapper: ModelObjectWrapper = model_class_object.getattr(py, "__teo_object__")?.extract(py)?;
    Ok(wrapper.object.clone())
}

pub(crate) fn teo_transaction_ctx_from_py_ctx_object(py: Python<'_>, ctx_object: PyObject) -> PyResult<transaction::Ctx> {
    let wrapper: TransactionCtxWrapper = ctx_object.getattr(py, "__teo_transaction_ctx__")?.extract(py)?;
    Ok(wrapper.ctx.clone())
}

static INIT_ERROR_MESSAGE: &str = "class is not initialized";

unsafe fn generate_model_class_class(py: Python<'_>, name: &str) -> PyResult<PyObject> {
    let builtins = py.import("builtins")?;
    let py_type = builtins.getattr("type")?;
    let py_object = builtins.getattr("object")?;
    let dict = PyDict::new(py);
    dict.set_item("__module__", "teo.models")?;
    let init = PyCFunction::new_closure(py, Some("__init__"), Some(INIT_ERROR_MESSAGE), |args, _kwargs| {
        let slf = args.get_item(0)?;
        let initialized: bool = slf.getattr("__teo_initialized__")?.extract()?;
        if initialized {
            Ok(())
        } else {
            Err::<(), PyErr>(PyRuntimeError::new_err(INIT_ERROR_MESSAGE))
        }
    })?;
    dict.set_item("__init__", init)?;
    let result = py_type.call1((name, (py_object,), dict))?;
    let result_object = result.into_py(py);
    classes_mut().insert(name.to_owned(), result_object);
    Ok(result.into_py(py))
}

unsafe fn generate_model_object_class(py: Python<'_>, name: &str) -> PyResult<PyObject> {
    let builtins = py.import("builtins")?;
    let py_type = builtins.getattr("type")?;
    let py_object = builtins.getattr("object")?;
    let dict = PyDict::new(py);
    dict.set_item("__module__", "teo.models")?;
    let init = PyCFunction::new_closure(py, Some("__init__"), Some(INIT_ERROR_MESSAGE), |args, _kwargs| {
        let slf = args.get_item(0)?;
        let initialized: bool = slf.getattr("__teo_initialized__")?.extract()?;
        if initialized {
            Ok(())
        } else {
            Err::<(), PyErr>(PyRuntimeError::new_err(INIT_ERROR_MESSAGE))
        }
    })?;
    dict.set_item("__init__", init)?;
    let result = py_type.call1((name, (py_object,), dict))?;
    let result_object = result.into_py(py);
    objects_mut().insert(name.to_owned(), result_object);
    Ok(result.into_py(py))
}

unsafe fn generate_ctx_class(py: Python<'_>, name: &str) -> PyResult<PyObject> {
    let builtins = py.import("builtins")?;
    let py_type = builtins.getattr("type")?;
    let py_object = builtins.getattr("object")?;
    let dict = PyDict::new(py);
    dict.set_item("__module__", "teo.models")?;
    let init = PyCFunction::new_closure(py, Some("__init__"), None, |args, _kwargs| {
        let slf = args.get_item(0)?;
        let initialized: bool = slf.getattr("__teo_initialized__")?.extract()?;
        if initialized {
            Ok(())
        } else {
            Err::<(), PyErr>(PyRuntimeError::new_err(INIT_ERROR_MESSAGE))
        }
    })?;
    dict.set_item("__init__", init)?;
    let result = py_type.call1((name, (py_object,), dict))?;
    let result_object = result.into_py(py);
    ctxs_mut().insert(name.to_owned(), result_object);
    Ok(result.into_py(py))
}

pub(crate) fn synthesize_dynamic_python_classes(py: Python<'_>, app: &App) -> PyResult<()> {
    synthesize_dynamic_nodejs_classes_for_namespace(py, app.main_namespace())
}

pub(crate) fn synthesize_dynamic_nodejs_classes_for_namespace(py: Python<'_>, namespace: &'static Namespace) -> PyResult<()> {
    synthesize_direct_dynamic_nodejs_classes_for_namespace(py, namespace)?;
    for namespace in namespace.namespaces.values() {
        synthesize_dynamic_nodejs_classes_for_namespace(py, namespace)?;
    }
    Ok(())
}

fn synthesize_direct_dynamic_nodejs_classes_for_namespace(py: Python<'_>, namespace: &'static Namespace) -> PyResult<()> {
    let main = py.import("__main__")?;
    let teo_wrap_builtin = main.getattr("teo_wrap_builtin")?;
    let builtins = py.import("builtins")?;
    let property_wrapper = builtins.getattr("property")?;
    let ctx_class = get_ctx_class(py, &namespace.path().join("."))?;
    for model in namespace.models.values() {
        let model_name = Box::leak(Box::new(model.path().join("."))).as_str();
        let model_property_name = model.path().last().unwrap().to_snake_case();
        let model_property = PyCFunction::new_closure(py, Some(model_name), None, move |args, _kwargs| {
            let model_class_object = Python::with_gil(|py| {
                let slf = args.get_item(0)?;
                let transaction_ctx_wrapper: TransactionCtxWrapper = slf.getattr("__teo_transaction_ctx__")?.extract()?;
                let model_ctx = transaction_ctx_wrapper.ctx.model_ctx_for_model_at_path(&model.path()).unwrap();
                let model_class_class = get_model_class_class(py, &model_name)?;
                let model_class_object = model_class_class.call_method1(py, "__new__", (model_class_class.as_ref(py),))?;
                model_class_object.setattr(py, "__teo_model_ctx__", ModelCtxWrapper::new(model_ctx))?;
                Ok::<PyObject, PyErr>(model_class_object)
            })?;
            Ok::<PyObject, PyErr>(model_class_object)
        })?;
        let model_property_wrapped = property_wrapper.call1((model_property,))?;
        ctx_class.setattr(py, model_property_name.as_str(), model_property_wrapped)?;
        // class object methods
        let model_class_class = get_model_class_class(py, &model_name)?;
        // find unique
        let find_unique = find_unique_function(py)?;
        teo_wrap_builtin.call1((model_class_class.as_ref(py), "find_unique", find_unique))?;
        // find first
        let find_first = find_first_function(py)?;
        teo_wrap_builtin.call1((model_class_class.as_ref(py), "find_first", find_first))?;
        // find many
        let find_many = find_many_function(py)?;
        teo_wrap_builtin.call1((model_class_class.as_ref(py), "find_many", find_many))?;
        // create
        let create = create_function(py)?;
        teo_wrap_builtin.call1((model_class_class.as_ref(py), "create", create))?;
        // count
        let count = count_function(py)?;
        teo_wrap_builtin.call1((model_class_class.as_ref(py), "count", count))?;
        // aggregate
        let aggregate = aggregate_function(py)?;
        teo_wrap_builtin.call1((model_class_class.as_ref(py), "aggregate", aggregate))?;
        // group by
        let group_by = group_by_function(py)?;
        teo_wrap_builtin.call1((model_class_class.as_ref(py), "group_by", group_by))?;
        // model object methods
        let model_object_class = get_model_object_class(py, &model_name)?;
        // is new
        let is_new = is_new_function(py)?;
        teo_wrap_builtin.call1((model_object_class.as_ref(py), "is_new", is_new))?;
        // is modified
        let is_modified = is_modified_function(py)?;
        teo_wrap_builtin.call1((model_object_class.as_ref(py), "is_modified", is_modified))?;
        // set
        let set = set_function(py)?;
        teo_wrap_builtin.call1((model_object_class.as_ref(py), "set", set))?;
        // update
        let update = update_function(py)?;
        teo_wrap_builtin.call1((model_object_class.as_ref(py), "update", update))?;
        // save
        let save = save_function(py)?;
        teo_wrap_builtin.call1((model_object_class.as_ref(py), "save", save))?;
        // delete
        let delete = delete_function(py)?;
        teo_wrap_builtin.call1((model_object_class.as_ref(py), "delete", delete))?;
        // to teon
        let to_teon = to_teon_function(py)?;
        teo_wrap_builtin.call1((model_object_class.as_ref(py), "to_teon", to_teon))?;
        // __repr__
        let repr = repr_function(py)?;
        teo_wrap_builtin.call1((model_object_class.as_ref(py), "__repr__", repr))?;

    }
    for namespace in namespace.namespaces.values() {
        let namespace_name = Box::leak(Box::new(namespace.path().join("."))).as_str();
        let namespace_property = PyCFunction::new_closure(py, Some(namespace_name), None, move |args, _kwargs| {
            let next_ctx_object = Python::with_gil(|py| {
                let slf = args.get_item(0)?;
                let transaction_ctx_wrapper: TransactionCtxWrapper = slf.getattr("__teo_transaction_ctx__")?.extract()?;
                let next_ctx_class = get_ctx_class(py, &namespace_name)?;
                let next_ctx_object = next_ctx_class.call_method1(py, "__new__", (next_ctx_class.as_ref(py),))?;
                next_ctx_object.setattr(py, "__teo_transaction_ctx__", transaction_ctx_wrapper.clone())?;
                Ok::<PyObject, PyErr>(next_ctx_object)
            })?;
            Ok::<PyObject, PyErr>(next_ctx_object)
        })?;
        let namespace_property_wrapped = property_wrapper.call1((namespace_property,))?;
        ctx_class.setattr(py, namespace.path().last().unwrap().to_snake_case().as_str(), namespace_property_wrapped)?;
    }
    // TODO: transaction
    ctx_class.setattr(py, "__teo_initialized__", true)?;
    Ok(())
}


fn find_unique_function<'py>(py: Python<'py>) -> PyResult<&'py PyCFunction> {
    Ok(PyCFunction::new_closure(py, Some("find_unique"), Some("Find a unique record."), move |args, _kwargs| {
        Python::with_gil(|py| {
            let slf = args.get_item(0)?.into_py(py);
            let model_ctx_wrapper: ModelCtxWrapper = slf.getattr(py, "__teo_model_ctx__")?.extract(py)?;
            let find_many_arg = if args.len() > 1 {
                let py_dict = args.get_item(1)?;
                check_py_dict(py_dict)?;
                py_any_to_teo_value(py, py_dict)?
            } else {
                Value::Dictionary(IndexMap::new())
            };
            let coroutine = pyo3_asyncio::tokio::future_into_py(py, (|| async move {
                let result: Option<model::Object> = model_ctx_wrapper.ctx.find_unique(&find_many_arg).await.into_py_result_with_gil()?;
                Python::with_gil(|py| {
                    match result {
                        Some(object) => {
                            py_model_object_from_teo_model_object(py, object)
                        }
                        None => {
                            Ok(().into_py(py))
                        }
                    }
                })
            })())?;
            Ok::<PyObject, PyErr>(coroutine.into_py(py))
        })
    })?)
}

fn find_first_function<'py>(py: Python<'py>) -> PyResult<&'py PyCFunction> {
    Ok(PyCFunction::new_closure(py, Some("find_first"), Some("Find a record."), move |args, _kwargs| {
        Python::with_gil(|py| {
            let slf = args.get_item(0)?.into_py(py);
            let model_ctx_wrapper: ModelCtxWrapper = slf.getattr(py, "__teo_model_ctx__")?.extract(py)?;
            let find_many_arg = if args.len() > 1 {
                let py_dict = args.get_item(1)?;
                check_py_dict(py_dict)?;
                py_any_to_teo_value(py, py_dict)?
            } else {
                Value::Dictionary(IndexMap::new())
            };
            let coroutine = pyo3_asyncio::tokio::future_into_py(py, (|| async move {
                let result: Option<model::Object> = model_ctx_wrapper.ctx.find_first(&find_many_arg).await.into_py_result_with_gil()?;
                Python::with_gil(|py| {
                    match result {
                        Some(object) => {
                            py_model_object_from_teo_model_object(py, object)
                        }
                        None => {
                            Ok(().into_py(py))
                        }
                    }
                })
            })())?;
            Ok::<PyObject, PyErr>(coroutine.into_py(py))
        })
    })?)
}

fn find_many_function<'py>(py: Python<'py>) -> PyResult<&'py PyCFunction> {
    Ok(PyCFunction::new_closure(py, Some("find_many"), Some("Find many records."), move |args, _kwargs| {
        Python::with_gil(|py| {
            let slf = args.get_item(0)?.into_py(py);
            let model_ctx_wrapper: ModelCtxWrapper = slf.getattr(py, "__teo_model_ctx__")?.extract(py)?;
            let find_many_arg = if args.len() > 1 {
                let py_dict = args.get_item(1)?;
                check_py_dict(py_dict)?;
                py_any_to_teo_value(py, py_dict)?
            } else {
                Value::Dictionary(IndexMap::new())
            };
            let coroutine = pyo3_asyncio::tokio::future_into_py::<_, PyObject>(py, (|| async move {
                let result: Vec<model::Object> = model_ctx_wrapper.ctx.find_many(&find_many_arg).await.into_py_result_with_gil()?;
                Python::with_gil(|py| {
                    let py_result = PyList::empty(py);
                    for object in result {
                        let instance = py_model_object_from_teo_model_object(py, object)?;
                        py_result.append(instance)?;
                    }
                    Ok(py_result.into_py(py))
                })
            })())?;
            Ok::<PyObject, PyErr>(coroutine.into_py(py))
        })
    })?)
}

fn create_function<'py>(py: Python<'py>) -> PyResult<&'py PyCFunction> {
    Ok(PyCFunction::new_closure(py, Some("create"), Some("Create a new record."), move |args, _kwargs| {
        Python::with_gil(|py| {
            let slf = args.get_item(0)?.into_py(py);
            let model_ctx_wrapper: ModelCtxWrapper = slf.getattr(py, "__teo_model_ctx__")?.extract(py)?;
            let create_arg = if args.len() > 1 {
                let py_dict = args.get_item(1)?;
                check_py_dict(py_dict)?;
                py_any_to_teo_value(py, py_dict)?
            } else {
                Value::Dictionary(IndexMap::new())
            };
            let coroutine = pyo3_asyncio::tokio::future_into_py::<_, PyObject>(py, (|| async move {
                let result: model::Object = model_ctx_wrapper.ctx.create_object(&create_arg).await.into_py_result_with_gil()?;
                Python::with_gil(|py| {
                    let instance = py_model_object_from_teo_model_object(py, result)?;
                    Ok(instance.into_py(py))
                })
            })())?;
            Ok::<PyObject, PyErr>(coroutine.into_py(py))
        })
    })?)
}

fn count_function<'py>(py: Python<'py>) -> PyResult<&'py PyCFunction> {
    Ok(PyCFunction::new_closure(py, Some("count"), Some("Count records."), move |args, _kwargs| {
        Python::with_gil(|py| {
            let slf = args.get_item(0)?.into_py(py);
            let model_ctx_wrapper: ModelCtxWrapper = slf.getattr(py, "__teo_model_ctx__")?.extract(py)?;
            let count_arg = if args.len() > 1 {
                let py_dict = args.get_item(1)?;
                check_py_dict(py_dict)?;
                py_any_to_teo_value(py, py_dict)?
            } else {
                Value::Dictionary(IndexMap::new())
            };
            let coroutine = pyo3_asyncio::tokio::future_into_py::<_, PyObject>(py, (|| async move {
                let result: usize = model_ctx_wrapper.ctx.count(&count_arg).await.into_py_result_with_gil()?;
                Python::with_gil(|py| {
                    Ok(result.into_py(py))
                })
            })())?;
            Ok::<PyObject, PyErr>(coroutine.into_py(py))
        })
    })?)
}

fn aggregate_function<'py>(py: Python<'py>) -> PyResult<&'py PyCFunction> {
    Ok(PyCFunction::new_closure(py, Some("aggregate"), Some("Aggregate on records."), move |args, _kwargs| {
        Python::with_gil(|py| {
            let slf = args.get_item(0)?.into_py(py);
            let model_ctx_wrapper: ModelCtxWrapper = slf.getattr(py, "__teo_model_ctx__")?.extract(py)?;
            let aggregate_arg = if args.len() > 1 {
                let py_dict = args.get_item(1)?;
                check_py_dict(py_dict)?;
                py_any_to_teo_value(py, py_dict)?
            } else {
                Value::Dictionary(IndexMap::new())
            };
            let coroutine = pyo3_asyncio::tokio::future_into_py::<_, PyObject>(py, (|| async move {
                let result: Value = model_ctx_wrapper.ctx.aggregate(&aggregate_arg).await.into_py_result_with_gil()?;
                Python::with_gil(|py| {
                    teo_value_to_py_any(py, &result)
                })
            })())?;
            Ok::<PyObject, PyErr>(coroutine.into_py(py))
        })
    })?)
}

fn group_by_function<'py>(py: Python<'py>) -> PyResult<&'py PyCFunction> {
    Ok(PyCFunction::new_closure(py, Some("group_by"), Some("Group by on records."), move |args, _kwargs| {
        Python::with_gil(|py| {
            let slf = args.get_item(0)?.into_py(py);
            let model_ctx_wrapper: ModelCtxWrapper = slf.getattr(py, "__teo_model_ctx__")?.extract(py)?;
            let group_by_arg = if args.len() > 1 {
                let py_dict = args.get_item(1)?;
                check_py_dict(py_dict)?;
                py_any_to_teo_value(py, py_dict)?
            } else {
                Value::Dictionary(IndexMap::new())
            };
            let coroutine = pyo3_asyncio::tokio::future_into_py::<_, PyObject>(py, (|| async move {
                let result: Vec<Value> = model_ctx_wrapper.ctx.group_by(&group_by_arg).await.into_py_result_with_gil()?;
                Python::with_gil(|py| {
                    let py_result = PyList::empty(py);
                    for value in result {
                        let instance = teo_value_to_py_any(py, &value)?;
                        py_result.append(instance)?;
                    }
                    Ok(py_result.into_py(py))
                })
            })())?;
            Ok::<PyObject, PyErr>(coroutine.into_py(py))
        })
    })?)
}

fn is_new_function<'py>(py: Python<'py>) -> PyResult<&'py PyCFunction> {
    Ok(PyCFunction::new_closure(py, Some("is_new"), Some("Whether this model object is new."), move |args, _kwargs| {
        Python::with_gil(|py| {
            let slf = args.get_item(0)?.into_py(py);
            let model_object_wrapper: ModelObjectWrapper = slf.getattr(py, "__teo_object__")?.extract(py)?;
            Ok::<PyObject, PyErr>(model_object_wrapper.object.is_new().into_py(py))
        })
    })?)
}

fn is_modified_function<'py>(py: Python<'py>) -> PyResult<&'py PyCFunction> {
    Ok(PyCFunction::new_closure(py, Some("is_modified"), Some("Whether this model object is modified."), move |args, _kwargs| {
        Python::with_gil(|py| {
            let slf = args.get_item(0)?.into_py(py);
            let model_object_wrapper: ModelObjectWrapper = slf.getattr(py, "__teo_object__")?.extract(py)?;
            Ok::<PyObject, PyErr>(model_object_wrapper.object.is_modified().into_py(py))
        })
    })?)
}

fn set_function<'py>(py: Python<'py>) -> PyResult<&'py PyCFunction> {
    Ok(PyCFunction::new_closure(py, Some("set"), Some("Set values to this object."), move |args, _kwargs| {
        Python::with_gil(|py| {
            let slf = args.get_item(0)?.into_py(py);
            let model_object_wrapper: ModelObjectWrapper = slf.getattr(py, "__teo_object__")?.extract(py)?;
            let set_arg = if args.len() > 1 {
                let py_dict = args.get_item(1)?;
                check_py_dict(py_dict)?;
                py_any_to_teo_value(py, py_dict)?
            } else {
                Value::Dictionary(IndexMap::new())
            };
            let coroutine = pyo3_asyncio::tokio::future_into_py::<_, PyObject>(py, (|| async move {
                let result: () = model_object_wrapper.object.set_teon(&set_arg).await.into_py_result_with_gil()?;
                Python::with_gil(|py| {
                    Ok(result.into_py(py))
                })
            })())?;
            Ok::<PyObject, PyErr>(coroutine.into_py(py))
        })
    })?)
}

fn update_function<'py>(py: Python<'py>) -> PyResult<&'py PyCFunction> {
    Ok(PyCFunction::new_closure(py, Some("update"), Some("Update values on this object."), move |args, _kwargs| {
        Python::with_gil(|py| {
            let slf = args.get_item(0)?.into_py(py);
            let model_object_wrapper: ModelObjectWrapper = slf.getattr(py, "__teo_object__")?.extract(py)?;
            let set_arg = if args.len() > 1 {
                let py_dict = args.get_item(1)?;
                check_py_dict(py_dict)?;
                py_any_to_teo_value(py, py_dict)?
            } else {
                Value::Dictionary(IndexMap::new())
            };
            let coroutine = pyo3_asyncio::tokio::future_into_py::<_, PyObject>(py, (|| async move {
                let result: () = model_object_wrapper.object.update_teon(&set_arg).await.into_py_result_with_gil()?;
                Python::with_gil(|py| {
                    Ok(result.into_py(py))
                })
            })())?;
            Ok::<PyObject, PyErr>(coroutine.into_py(py))
        })
    })?)
}

fn save_function<'py>(py: Python<'py>) -> PyResult<&'py PyCFunction> {
    Ok(PyCFunction::new_closure(py, Some("save"), Some("Save this object."), move |args, _kwargs| {
        Python::with_gil(|py| {
            let slf = args.get_item(0)?.into_py(py);
            let model_object_wrapper: ModelObjectWrapper = slf.getattr(py, "__teo_object__")?.extract(py)?;
            let coroutine = pyo3_asyncio::tokio::future_into_py::<_, PyObject>(py, (|| async move {
                let result: () = model_object_wrapper.object.save().await.into_py_result_with_gil()?;
                Python::with_gil(|py| {
                    Ok(result.into_py(py))
                })
            })())?;
            Ok::<PyObject, PyErr>(coroutine.into_py(py))
        })
    })?)
}

fn delete_function<'py>(py: Python<'py>) -> PyResult<&'py PyCFunction> {
    Ok(PyCFunction::new_closure(py, Some("delete"), Some("Delete this object."), move |args, _kwargs| {
        Python::with_gil(|py| {
            let slf = args.get_item(0)?.into_py(py);
            let model_object_wrapper: ModelObjectWrapper = slf.getattr(py, "__teo_object__")?.extract(py)?;
            let coroutine = pyo3_asyncio::tokio::future_into_py::<_, PyObject>(py, (|| async move {
                let result: () = model_object_wrapper.object.delete().await.into_py_result_with_gil()?;
                Python::with_gil(|py| {
                    Ok(result.into_py(py))
                })
            })())?;
            Ok::<PyObject, PyErr>(coroutine.into_py(py))
        })
    })?)
}

fn to_teon_function<'py>(py: Python<'py>) -> PyResult<&'py PyCFunction> {
    Ok(PyCFunction::new_closure(py, Some("to_teon"), Some("Convert this object to a Teon object."), move |args, _kwargs| {
        Python::with_gil(|py| {
            let slf = args.get_item(0)?.into_py(py);
            let model_object_wrapper: ModelObjectWrapper = slf.getattr(py, "__teo_object__")?.extract(py)?;
            let coroutine = pyo3_asyncio::tokio::future_into_py::<_, PyObject>(py, (|| async move {
                let result: Value = model_object_wrapper.object.to_teon().await.into_py_result_with_gil()?;
                Python::with_gil(|py| {
                    teo_value_to_py_any(py, &result)
                })
            })())?;
            Ok::<PyObject, PyErr>(coroutine.into_py(py))
        })
    })?)
}

fn repr_function<'py>(py: Python<'py>) -> PyResult<&'py PyCFunction> {
    Ok(PyCFunction::new_closure(py, Some("__repr__"), None, move |args, _kwargs| {
        Python::with_gil(|py| {
            let slf = args.get_item(0)?.into_py(py);
            let model_object_wrapper: ModelObjectWrapper = slf.getattr(py, "__teo_object__")?.extract(py)?;
            let result = PyDict::new(py);
            let value_map = model_object_wrapper.object.inner.value_map.lock().unwrap();
            for (k, v) in value_map.iter() {
                result.set_item(k, teo_value_to_py_any(py, v)?)?;
            }
            let dict_repr = result.call_method("__repr__", (), None)?;
            let dict_repr_str: &str = dict_repr.extract()?;
            let prefix = format!("{}(", model_object_wrapper.object.model().path().join("."));
            let suffix = ")";
            Ok::<PyObject, PyErr>(format!("{}{}{}", prefix, dict_repr_str, suffix).into_py(py))
        })
    })?)
}
