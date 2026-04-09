use magnolia_server::service::windows::{is_running_as_service, run_as_service};

fn main() {
    // Check if running as Windows service
    #[cfg(windows)]
    {
        if is_running_as_service() {
            // Running as Windows service - delegate to service handler
            if let Err(e) = run_as_service() {
                eprintln!("Service error: {:?}", e);
                std::process::exit(1);
            }
            return;
        }
    }

    // If not win service, run as console app (works the same on linux).
    // Spawn a thread with a generous stack — block_on() stores the entire
    // async future chain on the calling thread's OS stack, which overflows
    // the Windows default (1 MB) when running all migrations from scratch.
    // Overflow was fixed, but I'm still leaving the extra stach size,
    // because I haven't tested larger servers with extended traffic.
    let thread = std::thread::Builder::new()
        .name("magnolia-main".into())
        .stack_size(32 * 1024 * 1024) // 32 MB
        .spawn(|| {
            let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
            if let Err(e) = runtime.block_on(magnolia_server::run_server()) {
                eprintln!("Server error: {:?}", e);
                std::process::exit(1);
            }
        })
        .expect("Failed to spawn main thread");

    if let Err(e) = thread.join() {
        eprintln!("Main thread panicked: {:?}", e);
        std::process::exit(1);
    }
}
