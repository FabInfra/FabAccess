fn main() {
    ::capnpc::CompilerCommand::new().file("schema/api.capnp").run().unwrap()
}
