fn main() {
    vergen::EmitBuilder::builder().build_date().git_sha(true).emit().unwrap();
}
