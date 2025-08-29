use std::os::raw::c_char;
use std::ffi::{CString, CStr};
use std::collections::HashMap;
use lazy_static::lazy_static;
use serde_json::json;

mod providers;
use providers::CloudProvider;
use providers::aws::AwsProvider;

lazy_static! {
    static ref PROVIDER_REGISTRY: HashMap<&'static str, Box<dyn CloudProvider + Sync>> = {
        let mut m = HashMap::new();
        let aws_provider = AwsProvider;
        m.insert(aws_provider.name(), Box::new(aws_provider));
        m
    };
}

// Helper para parsear argumentos no formato --key value
fn parse_args(args: &[&str]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        if args[i].starts_with("--") {
            if i + 1 < args.len() {
                let key = args[i].trim_start_matches("--").to_string();
                let value = args[i+1].to_string();
                map.insert(key, value);
                i += 2;
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    map
}


#[no_mangle]
pub extern "C" fn ph_module_init() {}

#[no_mangle]
pub extern "C" fn ph_module_get_commands() -> *mut c_char {
    let commands_description = "aws list-vms [--region <value>], aws list-buckets, ...";
    CString::new(commands_description).unwrap().into_raw()
}

#[no_mangle]
pub extern "C" fn ph_module_exec(command_ptr: *const c_char, args_ptr: *const c_char) -> *mut c_char {
    let command_str = unsafe { CStr::from_ptr(command_ptr) }.to_str().unwrap_or("");
    let args_str = unsafe { CStr::from_ptr(args_ptr) }.to_str().unwrap_or("");

    let mut full_command: Vec<&str> = command_str.split_whitespace().collect();
    let additional_args: Vec<&str> = args_str.split_whitespace().collect();
    full_command.extend(additional_args);

    if full_command.len() < 2 {
        let err = json!({"error": "Comando cloud inválido. Formato: <provider> <sub-command> [args...]"});
        return CString::new(err.to_string()).unwrap().into_raw();
    }

    let provider_name = full_command[0];
    let sub_command = full_command[1];
    let command_args_map = parse_args(&full_command[2..]);

    let result = match PROVIDER_REGISTRY.get(provider_name) {
        Some(provider) => {
            let res = match sub_command {
                "list-vms" => provider.list_vms(&command_args_map),
                "list-buckets" => provider.list_buckets(&command_args_map),
                _ => Err(format!("Sub-comando desconhecido para {}: {}", provider_name, sub_command)),
            };
            match res {
                Ok(value) => value.to_string(),
                Err(e) => json!({"error": e}).to_string(),
            }
        },
        None => {
            json!({"error": format!("Provedor de nuvem desconhecido: {}", provider_name)}).to_string()
        }
    };

    CString::new(result).unwrap().into_raw()
}

#[no_mangle]
pub extern "C" fn ph_free_string(s: *mut c_char) {
    if s.is_null() { return }
    unsafe {
        let _ = CString::from_raw(s);
    }
}
