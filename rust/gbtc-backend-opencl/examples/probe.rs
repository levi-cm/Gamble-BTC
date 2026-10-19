fn main() {
    match gbtc_backend_opencl::OpenClBackend::new(None) {
        Ok(b) => println!("{:?}", b.backend_info()),
        Err(e) => eprintln!("{e}"),
    }
}
