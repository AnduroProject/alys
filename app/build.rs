fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Generate gRPC code from protobuf definitions
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir("src/generated")
        .compile(
            &["proto/governance/bridge/v1/governance.proto"],
            &["proto"],
        )?;

    Ok(())
}