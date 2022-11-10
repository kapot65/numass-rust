use protobuf_codegen::Codegen;

fn main() {
    // panic!("from build")
    Codegen::new()
        .pure()
        .cargo_out_dir("protos")
        .input("src/protos/schema.proto")
        .include("src/protos")
        .run_from_script();
}