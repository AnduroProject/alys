use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    
    // Configure tonic-build
    tonic_build::configure()
        .build_server(false) // We're only a client
        .build_client(true)  // Generate client code
        .out_dir(&out_dir)   // Output directory
        .compile(
            &["proto/governance.proto"], // Proto files
            &["proto/"],                  // Include directories  
        )?;

    // Tell Cargo to recompile if proto files change
    println!("cargo:rerun-if-changed=proto/governance.proto");
    println!("cargo:rerun-if-changed=proto/");
    
    Ok(())
}