extern crate wasmer_runtime;
extern crate wasmer_runtime_core;

use libc::{c_char, c_int, uint32_t, uint8_t};
use std::collections::HashMap;
use std::ffi::CStr;
use std::slice;
use std::sync::Arc;
use std::{ffi::c_void, ptr};
use wasmer_runtime::{Ctx, Global, ImportObject, Instance, Memory, Module, Table, Value};
use wasmer_runtime_core::export::{Context, Export, FuncPointer};
use wasmer_runtime_core::import::Namespace;
use wasmer_runtime_core::module::{ExportIndex, ImportName};
use wasmer_runtime_core::types::{FuncSig, Type};

pub mod error;
pub mod global;
pub mod memory;
pub mod module;
pub mod table;
pub mod value;

use error::{update_last_error, CApiError};
use global::wasmer_global_t;
use memory::wasmer_memory_t;
use module::wasmer_module_t;
use table::wasmer_table_t;
use value::{wasmer_value, wasmer_value_t, wasmer_value_tag};

#[repr(C)]
pub struct wasmer_instance_t;

#[repr(C)]
pub struct wasmer_instance_context_t;

#[allow(non_camel_case_types)]
#[repr(C)]
pub enum wasmer_result_t {
    WASMER_OK = 1,
    WASMER_ERROR = 2,
}

#[repr(C)]
#[derive(Clone)]
pub struct wasmer_import_func_t;

#[repr(C)]
#[derive(Clone)]
pub struct wasmer_export_func_t;

#[repr(C)]
pub struct wasmer_limits_t {
    pub min: uint32_t,
    pub max: wasmer_limit_option_t,
}

#[repr(C)]
pub struct wasmer_limit_option_t {
    pub has_some: bool,
    pub some: uint32_t,
}

#[repr(C)]
pub struct wasmer_import_t {
    module_name: wasmer_byte_array,
    import_name: wasmer_byte_array,
    tag: wasmer_import_export_kind,
    value: wasmer_import_export_value,
}

#[repr(C)]
#[derive(Clone)]
pub struct wasmer_import_descriptor_t;

#[repr(C)]
#[derive(Clone)]
pub struct wasmer_import_descriptors_t;

#[repr(C)]
#[derive(Clone)]
pub struct wasmer_export_t;

#[repr(C)]
#[derive(Clone)]
pub struct wasmer_exports_t;

#[repr(C)]
#[derive(Clone)]
pub struct wasmer_export_descriptor_t;

#[repr(C)]
#[derive(Clone)]
pub struct wasmer_export_descriptors_t;

#[allow(non_camel_case_types)]
#[repr(u32)]
#[derive(Clone)]
pub enum wasmer_import_export_kind {
    WASM_FUNCTION,
    WASM_GLOBAL,
    WASM_MEMORY,
    WASM_TABLE,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union wasmer_import_export_value {
    func: *const wasmer_import_func_t,
    table: *const wasmer_table_t,
    memory: *const wasmer_memory_t,
    global: *const wasmer_global_t,
}

#[repr(C)]
pub struct wasmer_byte_array {
    bytes: *const uint8_t,
    bytes_len: uint32_t,
}

/// Gets export descriptors for the given module
///
/// The caller owns the object and should call `wasmer_export_descriptors_destroy` to free it.
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub unsafe extern "C" fn wasmer_export_descriptors(
    module: *const wasmer_module_t,
    export_descriptors: *mut *mut wasmer_export_descriptors_t,
) {
    let module = &*(module as *const Module);

    let named_export_descriptors: Box<NamedExportDescriptors> = Box::new(NamedExportDescriptors(
        module.info().exports.iter().map(|e| e.into()).collect(),
    ));
    *export_descriptors =
        Box::into_raw(named_export_descriptors) as *mut wasmer_export_descriptors_t;
}

pub struct NamedExportDescriptors(Vec<NamedExportDescriptor>);

/// Frees the memory for the given export descriptors
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub extern "C" fn wasmer_export_descriptors_destroy(
    export_descriptors: *mut wasmer_export_descriptors_t,
) {
    if !export_descriptors.is_null() {
        unsafe { Box::from_raw(export_descriptors as *mut NamedExportDescriptors) };
    }
}

/// Gets the length of the export descriptors
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub unsafe extern "C" fn wasmer_export_descriptors_len(
    exports: *mut wasmer_export_descriptors_t,
) -> c_int {
    if exports.is_null() {
        return 0;
    }
    (*(exports as *mut NamedExportDescriptors)).0.len() as c_int
}

/// Gets export descriptor by index
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub unsafe extern "C" fn wasmer_export_descriptors_get(
    export_descriptors: *mut wasmer_export_descriptors_t,
    idx: c_int,
) -> *mut wasmer_export_descriptor_t {
    if export_descriptors.is_null() {
        return ptr::null_mut();
    }
    let named_export_descriptors = &mut *(export_descriptors as *mut NamedExportDescriptors);
    &mut (*named_export_descriptors).0[idx as usize] as *mut NamedExportDescriptor
        as *mut wasmer_export_descriptor_t
}

/// Gets name for the export descriptor
#[no_mangle]
#[allow(clippy::cast_ptr_alignment)]
pub unsafe extern "C" fn wasmer_export_descriptor_name(
    export_descriptor: *mut wasmer_export_descriptor_t,
) -> wasmer_byte_array {
    let named_export_descriptor = &*(export_descriptor as *mut NamedExportDescriptor);
    wasmer_byte_array {
        bytes: named_export_descriptor.name.as_ptr(),
        bytes_len: named_export_descriptor.name.len() as u32,
    }
}

/// Gets export descriptor kind
#[no_mangle]
#[allow(clippy::cast_ptr_alignment)]
pub unsafe extern "C" fn wasmer_export_descriptor_kind(
    export: *mut wasmer_export_descriptor_t,
) -> wasmer_import_export_kind {
    let named_export_descriptor = &*(export as *mut NamedExportDescriptor);
    named_export_descriptor.kind.clone()
}

/// Gets import descriptors for the given module
///
/// The caller owns the object and should call `wasmer_import_descriptors_destroy` to free it.
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub unsafe extern "C" fn wasmer_import_descriptors(
    module: *const wasmer_module_t,
    import_descriptors: *mut *mut wasmer_import_descriptors_t,
) {
    let module = &*(module as *const Module);
    let total_imports = module.info().imported_functions.len()
        + module.info().imported_tables.len()
        + module.info().imported_globals.len()
        + module.info().imported_memories.len();
    let mut descriptors: Vec<NamedImportDescriptor> = Vec::with_capacity(total_imports);

    for (
        _index,
        ImportName {
            namespace_index,
            name_index,
        },
    ) in &module.info().imported_functions
    {
        let namespace = module.info().namespace_table.get(*namespace_index);
        let name = module.info().name_table.get(*name_index);
        descriptors.push(NamedImportDescriptor {
            module: namespace.to_string(),
            name: name.to_string(),
            kind: wasmer_import_export_kind::WASM_FUNCTION,
        });
    }

    for (
        _index,
        (
            ImportName {
                namespace_index,
                name_index,
            },
            _,
        ),
    ) in &module.info().imported_tables
    {
        let namespace = module.info().namespace_table.get(*namespace_index);
        let name = module.info().name_table.get(*name_index);
        descriptors.push(NamedImportDescriptor {
            module: namespace.to_string(),
            name: name.to_string(),
            kind: wasmer_import_export_kind::WASM_TABLE,
        });
    }

    for (
        _index,
        (
            ImportName {
                namespace_index,
                name_index,
            },
            _,
        ),
    ) in &module.info().imported_globals
    {
        let namespace = module.info().namespace_table.get(*namespace_index);
        let name = module.info().name_table.get(*name_index);
        descriptors.push(NamedImportDescriptor {
            module: namespace.to_string(),
            name: name.to_string(),
            kind: wasmer_import_export_kind::WASM_GLOBAL,
        });
    }

    for (
        _index,
        (
            ImportName {
                namespace_index,
                name_index,
            },
            _,
        ),
    ) in &module.info().imported_memories
    {
        let namespace = module.info().namespace_table.get(*namespace_index);
        let name = module.info().name_table.get(*name_index);
        descriptors.push(NamedImportDescriptor {
            module: namespace.to_string(),
            name: name.to_string(),
            kind: wasmer_import_export_kind::WASM_MEMORY,
        });
    }

    let named_import_descriptors: Box<NamedImportDescriptors> =
        Box::new(NamedImportDescriptors(descriptors));
    *import_descriptors =
        Box::into_raw(named_import_descriptors) as *mut wasmer_import_descriptors_t;
}

pub struct NamedImportDescriptors(Vec<NamedImportDescriptor>);

/// Frees the memory for the given import descriptors
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub extern "C" fn wasmer_import_descriptors_destroy(
    import_descriptors: *mut wasmer_import_descriptors_t,
) {
    if !import_descriptors.is_null() {
        unsafe { Box::from_raw(import_descriptors as *mut NamedImportDescriptors) };
    }
}

/// Gets the length of the import descriptors
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub unsafe extern "C" fn wasmer_import_descriptors_len(
    exports: *mut wasmer_import_descriptors_t,
) -> c_int {
    if exports.is_null() {
        return 0;
    }
    (*(exports as *mut NamedImportDescriptors)).0.len() as c_int
}

/// Gets import descriptor by index
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub unsafe extern "C" fn wasmer_import_descriptors_get(
    import_descriptors: *mut wasmer_import_descriptors_t,
    idx: c_int,
) -> *mut wasmer_import_descriptor_t {
    if import_descriptors.is_null() {
        return ptr::null_mut();
    }
    let named_import_descriptors = &mut *(import_descriptors as *mut NamedImportDescriptors);
    &mut (*named_import_descriptors).0[idx as usize] as *mut NamedImportDescriptor
        as *mut wasmer_import_descriptor_t
}

/// Gets name for the import descriptor
#[no_mangle]
#[allow(clippy::cast_ptr_alignment)]
pub unsafe extern "C" fn wasmer_import_descriptor_name(
    import_descriptor: *mut wasmer_import_descriptor_t,
) -> wasmer_byte_array {
    let named_import_descriptor = &*(import_descriptor as *mut NamedImportDescriptor);
    wasmer_byte_array {
        bytes: named_import_descriptor.name.as_ptr(),
        bytes_len: named_import_descriptor.name.len() as u32,
    }
}

/// Gets module name for the import descriptor
#[no_mangle]
#[allow(clippy::cast_ptr_alignment)]
pub unsafe extern "C" fn wasmer_import_descriptor_module_name(
    import_descriptor: *mut wasmer_import_descriptor_t,
) -> wasmer_byte_array {
    let named_import_descriptor = &*(import_descriptor as *mut NamedImportDescriptor);
    wasmer_byte_array {
        bytes: named_import_descriptor.module.as_ptr(),
        bytes_len: named_import_descriptor.module.len() as u32,
    }
}

/// Gets export descriptor kind
#[no_mangle]
#[allow(clippy::cast_ptr_alignment)]
pub unsafe extern "C" fn wasmer_import_descriptor_kind(
    export: *mut wasmer_import_descriptor_t,
) -> wasmer_import_export_kind {
    let named_import_descriptor = &*(export as *mut NamedImportDescriptor);
    named_import_descriptor.kind.clone()
}

/// Creates a new Instance from the given wasm bytes and imports.
///
/// Returns `wasmer_result_t::WASMER_OK` upon success.
///
/// Returns `wasmer_result_t::WASMER_ERROR` upon failure. Use `wasmer_last_error_length`
/// and `wasmer_last_error_message` to get an error message.
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub unsafe extern "C" fn wasmer_instantiate(
    instance: *mut *mut wasmer_instance_t,
    wasm_bytes: *mut uint8_t,
    wasm_bytes_len: uint32_t,
    imports: *mut wasmer_import_t,
    imports_len: c_int,
) -> wasmer_result_t {
    if wasm_bytes.is_null() {
        update_last_error(CApiError {
            msg: "wasm bytes ptr is null".to_string(),
        });
        return wasmer_result_t::WASMER_ERROR;
    }
    let imports: &[wasmer_import_t] = slice::from_raw_parts(imports, imports_len as usize);
    let mut import_object = ImportObject::new();
    let mut namespaces = HashMap::new();
    for import in imports {
        let module_name = slice::from_raw_parts(
            import.module_name.bytes,
            import.module_name.bytes_len as usize,
        );
        let module_name = if let Ok(s) = std::str::from_utf8(module_name) {
            s
        } else {
            update_last_error(CApiError {
                msg: "error converting module name to string".to_string(),
            });
            return wasmer_result_t::WASMER_ERROR;
        };
        let import_name = slice::from_raw_parts(
            import.import_name.bytes,
            import.import_name.bytes_len as usize,
        );
        let import_name = if let Ok(s) = std::str::from_utf8(import_name) {
            s
        } else {
            update_last_error(CApiError {
                msg: "error converting import_name to string".to_string(),
            });
            return wasmer_result_t::WASMER_ERROR;
        };

        let namespace = namespaces.entry(module_name).or_insert_with(Namespace::new);

        let export = match import.tag {
            wasmer_import_export_kind::WASM_MEMORY => {
                let mem = import.value.memory as *mut Memory;
                Export::Memory((&*mem).clone())
            }
            wasmer_import_export_kind::WASM_FUNCTION => {
                let func_export = import.value.func as *mut Export;
                (&*func_export).clone()
            }
            wasmer_import_export_kind::WASM_GLOBAL => {
                let global = import.value.global as *mut Global;
                Export::Global((&*global).clone())
            }
            wasmer_import_export_kind::WASM_TABLE => {
                let table = import.value.table as *mut Table;
                Export::Table((&*table).clone())
            }
        };
        namespace.insert(import_name, export);
    }
    for (module_name, namespace) in namespaces.into_iter() {
        import_object.register(module_name, namespace);
    }

    let bytes: &[u8] = slice::from_raw_parts_mut(wasm_bytes, wasm_bytes_len as usize);
    let result = wasmer_runtime::instantiate(bytes, &import_object);
    let new_instance = match result {
        Ok(instance) => instance,
        Err(_error) => {
            // TODO the trait bound `wasmer_runtime::error::Error: std::error::Error` is not satisfied
            //update_last_error(error);
            update_last_error(CApiError {
                msg: "error instantiating".to_string(),
            });
            return wasmer_result_t::WASMER_ERROR;
        }
    };
    *instance = Box::into_raw(Box::new(new_instance)) as *mut wasmer_instance_t;
    wasmer_result_t::WASMER_OK
}

/// Calls an instances exported function by `name` with the provided parameters.
/// Results are set using the provided `results` pointer.
///
/// Returns `wasmer_result_t::WASMER_OK` upon success.
///
/// Returns `wasmer_result_t::WASMER_ERROR` upon failure. Use `wasmer_last_error_length`
/// and `wasmer_last_error_message` to get an error message.
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub unsafe extern "C" fn wasmer_instance_call(
    instance: *mut wasmer_instance_t,
    name: *const c_char,
    params: *const wasmer_value_t,
    params_len: c_int,
    results: *mut wasmer_value_t,
    results_len: c_int,
) -> wasmer_result_t {
    if instance.is_null() {
        update_last_error(CApiError {
            msg: "instance ptr is null".to_string(),
        });
        return wasmer_result_t::WASMER_ERROR;
    }
    if name.is_null() {
        update_last_error(CApiError {
            msg: "name ptr is null".to_string(),
        });
        return wasmer_result_t::WASMER_ERROR;
    }
    if params.is_null() {
        update_last_error(CApiError {
            msg: "params ptr is null".to_string(),
        });
        return wasmer_result_t::WASMER_ERROR;
    }

    let params: &[wasmer_value_t] = slice::from_raw_parts(params, params_len as usize);
    let params: Vec<Value> = params.iter().cloned().map(|x| x.into()).collect();

    let func_name_c = CStr::from_ptr(name);
    let func_name_r = func_name_c.to_str().unwrap();

    let results: &mut [wasmer_value_t] = slice::from_raw_parts_mut(results, results_len as usize);
    let result = (&*(instance as *mut Instance)).call(func_name_r, &params[..]);

    match result {
        Ok(results_vec) => {
            if !results_vec.is_empty() {
                let ret = match results_vec[0] {
                    Value::I32(x) => wasmer_value_t {
                        tag: wasmer_value_tag::WASM_I32,
                        value: wasmer_value { I32: x },
                    },
                    Value::I64(x) => wasmer_value_t {
                        tag: wasmer_value_tag::WASM_I64,
                        value: wasmer_value { I64: x },
                    },
                    Value::F32(x) => wasmer_value_t {
                        tag: wasmer_value_tag::WASM_F32,
                        value: wasmer_value { F32: x },
                    },
                    Value::F64(x) => wasmer_value_t {
                        tag: wasmer_value_tag::WASM_F64,
                        value: wasmer_value { F64: x },
                    },
                };
                results[0] = ret;
            }
            wasmer_result_t::WASMER_OK
        }
        Err(err) => {
            update_last_error(err);
            wasmer_result_t::WASMER_ERROR
        }
    }
}

/// Gets Exports for the given instance
///
/// The caller owns the object and should call `wasmer_exports_destroy` to free it.
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub unsafe extern "C" fn wasmer_instance_exports(
    instance: *mut wasmer_instance_t,
    exports: *mut *mut wasmer_exports_t,
) {
    let instance_ref = &mut *(instance as *mut Instance);
    let mut exports_vec: Vec<NamedExport> = Vec::with_capacity(instance_ref.exports().count());
    for (name, export) in instance_ref.exports() {
        exports_vec.push(NamedExport {
            name: name.clone(),
            export: export.clone(),
            instance: instance as *mut Instance,
        });
    }
    let named_exports: Box<NamedExports> = Box::new(NamedExports(exports_vec));
    *exports = Box::into_raw(named_exports) as *mut wasmer_exports_t;
}

/// Sets the `data` field of the instance context. This context will be
/// passed to all imported function for instance.
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub extern "C" fn wasmer_instance_context_data_set(
    instance: *mut wasmer_instance_t,
    data_ptr: *mut c_void,
) {
    let instance_ref = unsafe { &mut *(instance as *mut Instance) };
    instance_ref.context_mut().data = data_ptr;
}

pub struct NamedExports(Vec<NamedExport>);

/// Frees the memory for the given exports
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub extern "C" fn wasmer_exports_destroy(exports: *mut wasmer_exports_t) {
    if !exports.is_null() {
        unsafe { Box::from_raw(exports as *mut NamedExports) };
    }
}

/// Gets the length of the exports
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub unsafe extern "C" fn wasmer_exports_len(exports: *mut wasmer_exports_t) -> c_int {
    if exports.is_null() {
        return 0;
    }
    (*(exports as *mut NamedExports)).0.len() as c_int
}

/// Gets wasmer_export by index
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub unsafe extern "C" fn wasmer_exports_get(
    exports: *mut wasmer_exports_t,
    idx: c_int,
) -> *mut wasmer_export_t {
    if exports.is_null() {
        return ptr::null_mut();
    }
    let named_exports = &mut *(exports as *mut NamedExports);
    &mut (*named_exports).0[idx as usize] as *mut NamedExport as *mut wasmer_export_t
}

/// Gets wasmer_export kind
#[no_mangle]
#[allow(clippy::cast_ptr_alignment)]
pub unsafe extern "C" fn wasmer_export_kind(
    export: *mut wasmer_export_t,
) -> wasmer_import_export_kind {
    let named_export = &*(export as *mut NamedExport);
    match named_export.export {
        Export::Table(_) => wasmer_import_export_kind::WASM_TABLE,
        Export::Function { .. } => wasmer_import_export_kind::WASM_FUNCTION,
        Export::Global(_) => wasmer_import_export_kind::WASM_GLOBAL,
        Export::Memory(_) => wasmer_import_export_kind::WASM_MEMORY,
    }
}

/// Sets the result parameter to the arity of the params of the wasmer_export_func_t
///
/// Returns `wasmer_result_t::WASMER_OK` upon success.
///
/// Returns `wasmer_result_t::WASMER_ERROR` upon failure. Use `wasmer_last_error_length`
/// and `wasmer_last_error_message` to get an error message.
#[no_mangle]
#[allow(clippy::cast_ptr_alignment)]
pub unsafe extern "C" fn wasmer_export_func_params_arity(
    func: *const wasmer_export_func_t,
    result: *mut uint32_t,
) -> wasmer_result_t {
    let named_export = &*(func as *const NamedExport);
    let export = &named_export.export;
    if let Export::Function { ref signature, .. } = *export {
        *result = signature.params().len() as uint32_t;
        wasmer_result_t::WASMER_OK
    } else {
        update_last_error(CApiError {
            msg: "func ptr error in wasmer_export_func_params_arity".to_string(),
        });
        wasmer_result_t::WASMER_ERROR
    }
}

/// Sets the params buffer to the parameter types of the given wasmer_export_func_t
///
/// Returns `wasmer_result_t::WASMER_OK` upon success.
///
/// Returns `wasmer_result_t::WASMER_ERROR` upon failure. Use `wasmer_last_error_length`
/// and `wasmer_last_error_message` to get an error message.
#[no_mangle]
#[allow(clippy::cast_ptr_alignment)]
pub unsafe extern "C" fn wasmer_export_func_params(
    func: *const wasmer_export_func_t,
    params: *mut wasmer_value_tag,
    params_len: c_int,
) -> wasmer_result_t {
    let named_export = &*(func as *const NamedExport);
    let export = &named_export.export;
    if let Export::Function { ref signature, .. } = *export {
        let params: &mut [wasmer_value_tag] =
            slice::from_raw_parts_mut(params, params_len as usize);
        for (i, item) in signature.params().iter().enumerate() {
            params[i] = item.into();
        }
        wasmer_result_t::WASMER_OK
    } else {
        update_last_error(CApiError {
            msg: "func ptr error in wasmer_export_func_params".to_string(),
        });
        wasmer_result_t::WASMER_ERROR
    }
}

/// Sets the returns buffer to the parameter types of the given wasmer_export_func_t
///
/// Returns `wasmer_result_t::WASMER_OK` upon success.
///
/// Returns `wasmer_result_t::WASMER_ERROR` upon failure. Use `wasmer_last_error_length`
/// and `wasmer_last_error_message` to get an error message.
#[no_mangle]
#[allow(clippy::cast_ptr_alignment)]
pub unsafe extern "C" fn wasmer_export_func_returns(
    func: *const wasmer_export_func_t,
    returns: *mut wasmer_value_tag,
    returns_len: c_int,
) -> wasmer_result_t {
    let named_export = &*(func as *const NamedExport);
    let export = &named_export.export;
    if let Export::Function { ref signature, .. } = *export {
        let returns: &mut [wasmer_value_tag] =
            slice::from_raw_parts_mut(returns, returns_len as usize);
        for (i, item) in signature.returns().iter().enumerate() {
            returns[i] = item.into();
        }
        wasmer_result_t::WASMER_OK
    } else {
        update_last_error(CApiError {
            msg: "func ptr error in wasmer_export_func_returns".to_string(),
        });
        wasmer_result_t::WASMER_ERROR
    }
}

/// Sets the result parameter to the arity of the returns of the wasmer_export_func_t
///
/// Returns `wasmer_result_t::WASMER_OK` upon success.
///
/// Returns `wasmer_result_t::WASMER_ERROR` upon failure. Use `wasmer_last_error_length`
/// and `wasmer_last_error_message` to get an error message.
#[no_mangle]
#[allow(clippy::cast_ptr_alignment)]
pub unsafe extern "C" fn wasmer_export_func_returns_arity(
    func: *const wasmer_export_func_t,
    result: *mut uint32_t,
) -> wasmer_result_t {
    let named_export = &*(func as *const NamedExport);
    let export = &named_export.export;
    if let Export::Function { ref signature, .. } = *export {
        *result = signature.returns().len() as uint32_t;
        wasmer_result_t::WASMER_OK
    } else {
        update_last_error(CApiError {
            msg: "func ptr error in wasmer_export_func_results_arity".to_string(),
        });
        wasmer_result_t::WASMER_ERROR
    }
}

/// Sets the result parameter to the arity of the params of the wasmer_import_func_t
///
/// Returns `wasmer_result_t::WASMER_OK` upon success.
///
/// Returns `wasmer_result_t::WASMER_ERROR` upon failure. Use `wasmer_last_error_length`
/// and `wasmer_last_error_message` to get an error message.
#[no_mangle]
#[allow(clippy::cast_ptr_alignment)]
pub unsafe extern "C" fn wasmer_import_func_params_arity(
    func: *const wasmer_import_func_t,
    result: *mut uint32_t,
) -> wasmer_result_t {
    let export = &*(func as *const Export);
    if let Export::Function { ref signature, .. } = *export {
        *result = signature.params().len() as uint32_t;
        wasmer_result_t::WASMER_OK
    } else {
        update_last_error(CApiError {
            msg: "func ptr error in wasmer_import_func_params_arity".to_string(),
        });
        wasmer_result_t::WASMER_ERROR
    }
}

/// Creates new func
///
/// The caller owns the object and should call `wasmer_import_func_destroy` to free it.
#[no_mangle]
#[allow(clippy::cast_ptr_alignment)]
pub unsafe extern "C" fn wasmer_import_func_new(
    func: extern "C" fn(data: *mut c_void),
    params: *const wasmer_value_tag,
    params_len: c_int,
    returns: *const wasmer_value_tag,
    returns_len: c_int,
) -> *mut wasmer_import_func_t {
    let params: &[wasmer_value_tag] = slice::from_raw_parts(params, params_len as usize);
    let params: Vec<Type> = params.iter().cloned().map(|x| x.into()).collect();
    let returns: &[wasmer_value_tag] = slice::from_raw_parts(returns, returns_len as usize);
    let returns: Vec<Type> = returns.iter().cloned().map(|x| x.into()).collect();

    let export = Box::new(Export::Function {
        func: FuncPointer::new(func as _),
        ctx: Context::Internal,
        signature: Arc::new(FuncSig::new(params, returns)),
    });
    Box::into_raw(export) as *mut wasmer_import_func_t
}

/// Sets the params buffer to the parameter types of the given wasmer_import_func_t
///
/// Returns `wasmer_result_t::WASMER_OK` upon success.
///
/// Returns `wasmer_result_t::WASMER_ERROR` upon failure. Use `wasmer_last_error_length`
/// and `wasmer_last_error_message` to get an error message.
#[no_mangle]
#[allow(clippy::cast_ptr_alignment)]
pub unsafe extern "C" fn wasmer_import_func_params(
    func: *const wasmer_import_func_t,
    params: *mut wasmer_value_tag,
    params_len: c_int,
) -> wasmer_result_t {
    let export = &*(func as *const Export);
    if let Export::Function { ref signature, .. } = *export {
        let params: &mut [wasmer_value_tag] =
            slice::from_raw_parts_mut(params, params_len as usize);
        for (i, item) in signature.params().iter().enumerate() {
            params[i] = item.into();
        }
        wasmer_result_t::WASMER_OK
    } else {
        update_last_error(CApiError {
            msg: "func ptr error in wasmer_import_func_params".to_string(),
        });
        wasmer_result_t::WASMER_ERROR
    }
}

/// Sets the returns buffer to the parameter types of the given wasmer_import_func_t
///
/// Returns `wasmer_result_t::WASMER_OK` upon success.
///
/// Returns `wasmer_result_t::WASMER_ERROR` upon failure. Use `wasmer_last_error_length`
/// and `wasmer_last_error_message` to get an error message.
#[no_mangle]
#[allow(clippy::cast_ptr_alignment)]
pub unsafe extern "C" fn wasmer_import_func_returns(
    func: *const wasmer_import_func_t,
    returns: *mut wasmer_value_tag,
    returns_len: c_int,
) -> wasmer_result_t {
    let export = &*(func as *const Export);
    if let Export::Function { ref signature, .. } = *export {
        let returns: &mut [wasmer_value_tag] =
            slice::from_raw_parts_mut(returns, returns_len as usize);
        for (i, item) in signature.returns().iter().enumerate() {
            returns[i] = item.into();
        }
        wasmer_result_t::WASMER_OK
    } else {
        update_last_error(CApiError {
            msg: "func ptr error in wasmer_import_func_returns".to_string(),
        });
        wasmer_result_t::WASMER_ERROR
    }
}

/// Sets the result parameter to the arity of the returns of the wasmer_import_func_t
///
/// Returns `wasmer_result_t::WASMER_OK` upon success.
///
/// Returns `wasmer_result_t::WASMER_ERROR` upon failure. Use `wasmer_last_error_length`
/// and `wasmer_last_error_message` to get an error message.
#[no_mangle]
#[allow(clippy::cast_ptr_alignment)]
pub unsafe extern "C" fn wasmer_import_func_returns_arity(
    func: *const wasmer_import_func_t,
    result: *mut uint32_t,
) -> wasmer_result_t {
    let export = &*(func as *const Export);
    if let Export::Function { ref signature, .. } = *export {
        *result = signature.returns().len() as uint32_t;
        wasmer_result_t::WASMER_OK
    } else {
        update_last_error(CApiError {
            msg: "func ptr error in wasmer_import_func_results_arity".to_string(),
        });
        wasmer_result_t::WASMER_ERROR
    }
}

/// Frees memory for the given Func
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub extern "C" fn wasmer_import_func_destroy(func: *mut wasmer_import_func_t) {
    if !func.is_null() {
        unsafe { Box::from_raw(func as *mut Export) };
    }
}

/// Gets export func from export
#[no_mangle]
#[allow(clippy::cast_ptr_alignment)]
pub unsafe extern "C" fn wasmer_export_to_func(
    export: *const wasmer_export_t,
) -> *const wasmer_export_func_t {
    export as *const wasmer_export_func_t
}

/// Gets a memory pointer from an export pointer.
///
/// Returns `wasmer_result_t::WASMER_OK` upon success.
///
/// Returns `wasmer_result_t::WASMER_ERROR` upon failure. Use `wasmer_last_error_length`
/// and `wasmer_last_error_message` to get an error message.
#[no_mangle]
#[allow(clippy::cast_ptr_alignment)]
pub unsafe extern "C" fn wasmer_export_to_memory(
    export: *const wasmer_export_t,
    memory: *mut *mut wasmer_memory_t,
) -> wasmer_result_t {
    let named_export = &*(export as *const NamedExport);
    let export = &named_export.export;

    if let Export::Memory(exported_memory) = export {
        *memory = exported_memory as *const Memory as *mut wasmer_memory_t;
        wasmer_result_t::WASMER_OK
    } else {
        update_last_error(CApiError {
            msg: "cannot cast the `wasmer_export_t` pointer to a  `wasmer_memory_t` \
                  pointer because it does not represent a memory export."
                .to_string(),
        });
        wasmer_result_t::WASMER_ERROR
    }
}

/// Gets name from wasmer_export
#[no_mangle]
#[allow(clippy::cast_ptr_alignment)]
pub unsafe extern "C" fn wasmer_export_name(export: *mut wasmer_export_t) -> wasmer_byte_array {
    let named_export = &*(export as *mut NamedExport);
    wasmer_byte_array {
        bytes: named_export.name.as_ptr(),
        bytes_len: named_export.name.len() as u32,
    }
}

/// Calls a `func` with the provided parameters.
/// Results are set using the provided `results` pointer.
///
/// Returns `wasmer_result_t::WASMER_OK` upon success.
///
/// Returns `wasmer_result_t::WASMER_ERROR` upon failure. Use `wasmer_last_error_length`
/// and `wasmer_last_error_message` to get an error message.
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub unsafe extern "C" fn wasmer_export_func_call(
    func: *const wasmer_export_func_t,
    params: *const wasmer_value_t,
    params_len: c_int,
    results: *mut wasmer_value_t,
    results_len: c_int,
) -> wasmer_result_t {
    if func.is_null() {
        update_last_error(CApiError {
            msg: "func ptr is null".to_string(),
        });
        return wasmer_result_t::WASMER_ERROR;
    }
    if params.is_null() {
        update_last_error(CApiError {
            msg: "params ptr is null".to_string(),
        });
        return wasmer_result_t::WASMER_ERROR;
    }

    let params: &[wasmer_value_t] = slice::from_raw_parts(params, params_len as usize);
    let params: Vec<Value> = params.iter().cloned().map(|x| x.into()).collect();

    let named_export = &*(func as *mut NamedExport);

    let results: &mut [wasmer_value_t] = slice::from_raw_parts_mut(results, results_len as usize);

    let instance = &*named_export.instance;
    let result = instance.call(&named_export.name, &params[..]);
    match result {
        Ok(results_vec) => {
            if !results_vec.is_empty() {
                let ret = match results_vec[0] {
                    Value::I32(x) => wasmer_value_t {
                        tag: wasmer_value_tag::WASM_I32,
                        value: wasmer_value { I32: x },
                    },
                    Value::I64(x) => wasmer_value_t {
                        tag: wasmer_value_tag::WASM_I64,
                        value: wasmer_value { I64: x },
                    },
                    Value::F32(x) => wasmer_value_t {
                        tag: wasmer_value_tag::WASM_F32,
                        value: wasmer_value { F32: x },
                    },
                    Value::F64(x) => wasmer_value_t {
                        tag: wasmer_value_tag::WASM_F64,
                        value: wasmer_value { F64: x },
                    },
                };
                results[0] = ret;
            }
            wasmer_result_t::WASMER_OK
        }
        Err(err) => {
            update_last_error(err);
            wasmer_result_t::WASMER_ERROR
        }
    }
}

/// Gets the memory within the context at the index `memory_idx`.
/// The index is always 0 until multiple memories are supported.
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub extern "C" fn wasmer_instance_context_memory(
    ctx: *const wasmer_instance_context_t,
    _memory_idx: uint32_t,
) -> *const wasmer_memory_t {
    let ctx = unsafe { &*(ctx as *const Ctx) };
    let memory = ctx.memory(0);
    memory as *const Memory as *const wasmer_memory_t
}

/// Gets the `data` field within the context.
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub extern "C" fn wasmer_instance_context_data_get(
    ctx: *const wasmer_instance_context_t,
) -> *mut c_void {
    let ctx = unsafe { &*(ctx as *const Ctx) };
    ctx.data
}

/// Frees memory for the given Instance
#[allow(clippy::cast_ptr_alignment)]
#[no_mangle]
pub extern "C" fn wasmer_instance_destroy(instance: *mut wasmer_instance_t) {
    if !instance.is_null() {
        unsafe { Box::from_raw(instance as *mut Instance) };
    }
}

impl From<wasmer_value_t> for Value {
    fn from(v: wasmer_value_t) -> Self {
        unsafe {
            match v {
                wasmer_value_t {
                    tag: wasmer_value_tag::WASM_I32,
                    value: wasmer_value { I32 },
                } => Value::I32(I32),
                wasmer_value_t {
                    tag: wasmer_value_tag::WASM_I64,
                    value: wasmer_value { I64 },
                } => Value::I64(I64),
                wasmer_value_t {
                    tag: wasmer_value_tag::WASM_F32,
                    value: wasmer_value { F32 },
                } => Value::F32(F32),
                wasmer_value_t {
                    tag: wasmer_value_tag::WASM_F64,
                    value: wasmer_value { F64 },
                } => Value::F64(F64),
                _ => panic!("not implemented"),
            }
        }
    }
}

impl From<Value> for wasmer_value_t {
    fn from(val: Value) -> Self {
        match val {
            Value::I32(x) => wasmer_value_t {
                tag: wasmer_value_tag::WASM_I32,
                value: wasmer_value { I32: x },
            },
            Value::I64(x) => wasmer_value_t {
                tag: wasmer_value_tag::WASM_I64,
                value: wasmer_value { I64: x },
            },
            Value::F32(x) => wasmer_value_t {
                tag: wasmer_value_tag::WASM_F32,
                value: wasmer_value { F32: x },
            },
            Value::F64(x) => wasmer_value_t {
                tag: wasmer_value_tag::WASM_F64,
                value: wasmer_value { F64: x },
            },
        }
    }
}

impl From<Type> for wasmer_value_tag {
    fn from(ty: Type) -> Self {
        match ty {
            Type::I32 => wasmer_value_tag::WASM_I32,
            Type::I64 => wasmer_value_tag::WASM_I64,
            Type::F32 => wasmer_value_tag::WASM_F32,
            Type::F64 => wasmer_value_tag::WASM_F64,
            _ => panic!("not implemented"),
        }
    }
}

impl From<wasmer_value_tag> for Type {
    fn from(v: wasmer_value_tag) -> Self {
        match v {
            wasmer_value_tag::WASM_I32 => Type::I32,
            wasmer_value_tag::WASM_I64 => Type::I64,
            wasmer_value_tag::WASM_F32 => Type::F32,
            wasmer_value_tag::WASM_F64 => Type::F64,
            _ => panic!("not implemented"),
        }
    }
}

impl From<&wasmer_runtime::wasm::Type> for wasmer_value_tag {
    fn from(ty: &Type) -> Self {
        match *ty {
            Type::I32 => wasmer_value_tag::WASM_I32,
            Type::I64 => wasmer_value_tag::WASM_I64,
            Type::F32 => wasmer_value_tag::WASM_F32,
            Type::F64 => wasmer_value_tag::WASM_F64,
        }
    }
}

impl From<(&std::string::String, &ExportIndex)> for NamedExportDescriptor {
    fn from((name, export_index): (&String, &ExportIndex)) -> Self {
        let kind = match *export_index {
            ExportIndex::Memory(_) => wasmer_import_export_kind::WASM_MEMORY,
            ExportIndex::Global(_) => wasmer_import_export_kind::WASM_GLOBAL,
            ExportIndex::Table(_) => wasmer_import_export_kind::WASM_TABLE,
            ExportIndex::Func(_) => wasmer_import_export_kind::WASM_FUNCTION,
        };
        NamedExportDescriptor {
            name: name.clone(),
            kind,
        }
    }
}

struct NamedImportDescriptor {
    module: String,
    name: String,
    kind: wasmer_import_export_kind,
}

struct NamedExport {
    name: String,
    export: Export,
    instance: *mut Instance,
}

struct NamedExportDescriptor {
    name: String,
    kind: wasmer_import_export_kind,
}
