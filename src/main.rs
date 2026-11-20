fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime");
    let result = runtime.block_on(tezbar_extension_sdk::serve(
        tezbar_visual_studio_code::Port::default(),
    ));
    runtime.shutdown_timeout(std::time::Duration::from_secs(1));
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
