#[tokio::main(worker_threads = 2)]
async fn main() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        yaya_host_monitor::server::run().await
    }
    #[cfg(not(unix))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Host monitor requires Linux; no TCP or mock fallback is enabled",
        ))
    }
}
