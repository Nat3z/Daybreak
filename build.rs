fn main() {
    protobuf_codegen::Codegen::new()
        .cargo_out_dir("protos")
        .include("src")
        .protoc_extra_arg("--proto_path=src/protos")
        .inputs(&[
            "src/protos/device.proto",
            "src/protos/gamestate.proto",
            "src/protos/input.proto",
            "src/protos/run_mode.proto",
            "src/protos/runtime_status.proto",
            "src/protos/text.proto",
            "src/protos/timestamp.proto"
        ])
        .run_from_script();
}