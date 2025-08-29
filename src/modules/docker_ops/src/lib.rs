use std::os::raw::{c_char};
use std::ffi::{CString, CStr};
use std::process::Command;
use serde::{Serialize, Deserialize};

// Helper para executar comandos no shell
fn run_command(command: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(command)
        .args(args)
        .output();

    match output {
        Ok(output) => {
            if output.status.success() {
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            } else {
                Err(String::from_utf8_lossy(&output.stderr).to_string())
            }
        },
        Err(e) => Err(e.to_string()),
    }
}

// --- Estruturas para parsing do JSON do Docker ---

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DockerImage {
    #[serde(rename = "ID")]
    id: String,
    repository: String,
    tag: String,
    size: String,
    #[serde(rename = "CreatedSince")]
    created: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DockerContainer {
    #[serde(rename = "ID")]
    id: String,
    image: String,
    command: String,
    #[serde(rename = "CreatedAt")]
    created: String,
    status: String,
    ports: String,
    names: String,
}

// Função para parsear a saída JSON do Docker (um JSON por linha)
fn parse_docker_json_output<'a, T>(output: &'a str) -> Result<String, String>
where
    T: Deserialize<'a> + Serialize,
{
    let items: Result<Vec<T>, _> = output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line))
        .collect();

    match items {
        Ok(items) => serde_json::to_string_pretty(&items)
            .map_err(|e| format!("Falha ao serializar resultado para JSON: {}", e)),
        Err(e) => Err(format!("Falha ao parsear linha de JSON do Docker: {}", e)),
    }
}


// --- Funções da FFI ---

#[no_mangle]
pub extern "C" fn ph_module_init() {}

#[no_mangle]
pub extern "C" fn ph_module_get_commands() -> *mut c_char {
    let commands = "build,run,ps,images";
    CString::new(commands).unwrap().into_raw()
}

#[no_mangle]
pub extern "C" fn ph_module_exec(command_ptr: *const c_char, args_ptr: *const c_char) -> *mut c_char {
    let command_c_str = unsafe { CStr::from_ptr(command_ptr) };
    let command = command_c_str.to_str().unwrap_or("");

    let args_c_str = unsafe { CStr::from_ptr(args_ptr) };
    let args_str = args_c_str.to_str().unwrap_or("");
    let args: Vec<&str> = args_str.split_whitespace().collect();

    let result = match command {
        "images" => {
            match run_command("docker", &["images", "--format", "{{json .}}"]) {
                Ok(output) => parse_docker_json_output::<DockerImage>(&output).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e)),
                Err(e) => format!("{{\"error\":\"{}\"}}", e),
            }
        },
        "ps" => {
             match run_command("docker", &["ps", "-a", "--format", "{{json .}}"]) {
                Ok(output) => parse_docker_json_output::<DockerContainer>(&output).unwrap_or_else(|e| format!("{{\"error\":\"{}\"}}", e)),
                Err(e) => format!("{{\"error\":\"{}\"}}", e),
            }
        },
        "run" | "build" => {
            let mut docker_args = vec![command];
            docker_args.extend(args);
            match run_command("docker", &docker_args) {
                 Ok(output) => output,
                 Err(e) => format!("{{\"error\":\"{}\"}}", e),
            }
        },
        _ => format!("{{\"error\":\"Comando docker desconhecido: {}\"}}", command),
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
