fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 无本机 protoc 时使用 vendored 版本，避免环境依赖
    if std::env::var_os("PROTOC").is_none() {
        if let Ok(path) = protoc_bin_vendored::protoc_bin_path() {
            std::env::set_var("PROTOC", path);
        }
    }
    // tonic 0.14 起 prost 代码生成迁移到 tonic-prost-build
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                "proto/common.proto",
                "proto/adapter_link.proto",
                "proto/control.proto",
            ],
            &["proto"],
        )?;
    Ok(())
}
