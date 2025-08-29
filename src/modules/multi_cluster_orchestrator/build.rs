fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=../../ipc/schemas/rpc_data.proto");

    prost_build::compile_protos(&["../../ipc/schemas/rpc_data.proto"], &["../../ipc/schemas/"])?;
    
    Ok(())
}
