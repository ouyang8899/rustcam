fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 无本机 protoc 时使用 vendored 版本，避免环境依赖
    if std::env::var_os("PROTOC").is_none() {
        if let Ok(path) = protoc_bin_vendored::protoc_bin_path() {
            std::env::set_var("PROTOC", path);
        }
    }
    // tonic 0.14 起 prost 代码生成迁移到 tonic-prost-build
    // 描述集直接落在 OUT_DIR：prost-build 默认写到系统临时目录，
    // 受限的终端策略下会因「拒绝访问」导致构建失败。
    let out_dir = std::env::var("OUT_DIR")?;
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(
            std::path::Path::new(&out_dir).join("protos-descriptor-set.pb"),
        )
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
