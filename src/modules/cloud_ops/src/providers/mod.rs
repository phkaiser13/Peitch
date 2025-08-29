use serde_json::Value;
use std::collections::HashMap;

// O trait que define o contrato para qualquer provedor de nuvem suportado.
pub trait CloudProvider {
    // Retorna o nome do provedor (ex: "aws", "gcp")
    fn name(&self) -> &'static str;

    // As funções agora aceitam um mapa de argumentos opcionais.
    fn list_vms(&self, args: &HashMap<String, String>) -> Result<Value, String>;
    fn list_buckets(&self, args: &HashMap<String, String>) -> Result<Value, String>;
}

// Declara os submódulos de provedores.
pub mod aws;
