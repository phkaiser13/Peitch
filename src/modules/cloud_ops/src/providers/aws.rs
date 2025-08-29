use super::CloudProvider;
use serde_json::{Value, from_str};
use std::process::Command;
use std::collections::HashMap;

pub struct AwsProvider;

// Helper para executar comandos da AWS CLI e obter a saída em JSON
fn run_aws_command(service: &str, command: &str, cli_args: &[String]) -> Result<Value, String> {
    let mut final_args = vec![service, command];
    // A little verbose, but converts Vec<String> to Vec<&str> for Command::args
    let cli_args_str: Vec<&str> = cli_args.iter().map(|s| s.as_str()).collect();
    final_args.extend_from_slice(&cli_args_str);

    if !final_args.contains(&"--output") {
        final_args.push("--output");
        final_args.push("json");
    }

    let output = Command::new("aws")
        .args(&final_args)
        .output();
    
    match output {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                from_str(&stdout).map_err(|e| format!("Falha ao parsear JSON da AWS CLI: {}", e))
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("Erro ao executar comando AWS CLI: {}", stderr))
            }
        },
        Err(e) => Err(format!("Falha ao executar 'aws': {}. A AWS CLI está instalada e no PATH?", e)),
    }
}


impl CloudProvider for AwsProvider {
    fn name(&self) -> &'static str {
        "aws"
    }

    fn list_vms(&self, args: &HashMap<String, String>) -> Result<Value, String> {
        let mut cli_args = Vec::new();
        if let Some(region) = args.get("region") {
            cli_args.push("--region".to_string());
            cli_args.push(region.clone());
        }
        run_aws_command("ec2", "describe-instances", &cli_args)
    }

    fn list_buckets(&self, args: &HashMap<String, String>) -> Result<Value, String> {
        let mut cli_args = Vec::new();
        // O comando list-buckets não costuma usar region, mas o padrão é extensível
        if let Some(region) = args.get("region") {
             cli_args.push("--region".to_string());
             cli_args.push(region.clone());
        }
        run_aws_command("s3api", "list-buckets", &cli_args)
    }
}
