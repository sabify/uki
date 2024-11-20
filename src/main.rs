use cfg_if::cfg_if;
use clap::Parser;
use daemonize::Daemonize;

cfg_if! {
    if #[cfg(all(feature = "alloc-jem", not(target_env = "msvc")))] {
        use jemallocator::Jemalloc;
        #[global_allocator]
        static GLOBAL: Jemalloc = Jemalloc;
    } else if #[cfg(feature = "alloc-mim")] {
        use mimalloc::MiMalloc;
        #[global_allocator]
        static GLOBAL: MiMalloc = MiMalloc;
    }
}

fn main() {
    let cli = uki::args::Args::parse();

    if let Some(ref log_path) = cli.log_path {
        let log_file = std::fs::File::create(log_path)
            .unwrap_or_else(|_| panic!("could not create log file {}", log_path.to_string_lossy()));

        tracing_subscriber::fmt()
            .with_max_level(cli.log_level)
            .with_writer(log_file)
            .with_ansi(false)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(cli.log_level)
            .init();
    }

    if cli.daemonize {
        let daemonize = Daemonize::new().working_directory("/tmp");

        match daemonize.start() {
            Ok(_) => {}
            Err(e) => panic!("daemonize failed: {}", e),
        }
    }

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(main_async(cli))
        .unwrap();
}

async fn main_async(args: uki::args::Args) -> std::io::Result<()> {
    uki::handler::handle(args).await
}
