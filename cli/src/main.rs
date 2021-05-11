use {clap::Clap, tracing_subscriber::layer::SubscriberExt as _, treasury::Treasury, uuid::Uuid};

#[derive(Clap)]
#[clap(version = "0.1", author = "Zakarum <zakarumych@ya.ru>")]
struct Opts {
    /// Goods root directory path
    #[clap(short, long, default_value = ".")]
    root: String,

    /// A level of verbosity, and can be used multiple times
    #[clap(short, long, parse(from_occurrences))]
    verbose: i32,

    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Clap)]
enum SubCommand {
    Create(Create),
    Update(Update),
    Store(Store),
    Fetch(Fetch),
}

#[derive(Clap)]
struct Create {
    #[clap(short, long, default_value = ".")]
    importers_dir: String,
}

#[derive(Clap)]
struct Update {
    #[clap(short, long, default_value = ".")]
    importers_dir: String,
}

/// A subcommand for registering assets
#[derive(Clap)]
struct Store {
    /// Path to asset source file.
    #[clap()]
    source_path: String,

    /// Importer name.
    #[clap()]
    importer: String,
}

/// A subcommand for registering assets
#[derive(Clap)]
struct Fetch {
    /// Output binary or too long data
    #[clap(short, long)]
    binary: bool,

    /// Path to asset source file.
    #[clap()]
    uuid: Uuid,
}

fn main() -> eyre::Result<()> {
    if let Err(err) = color_eyre::install() {
        tracing::error!("Failed to install eyre report handler: {}", err);
    }

    let opts: Opts = Opts::parse();

    let level = match opts.verbose {
        0 => tracing::Level::WARN,
        1 => tracing::Level::INFO,
        2 => tracing::Level::DEBUG,
        3 | _ => tracing::Level::TRACE,
    };

    if let Err(err) = tracing::subscriber::set_global_default(
        tracing_subscriber::fmt()
            .with_max_level(level)
            .finish()
            .with(tracing_error::ErrorLayer::default()),
    ) {
        tracing::error!("Failed to install tracing subscriber: {}", err);
    }

    match opts.subcmd {
        SubCommand::Create(create) => {
            let mut treasury = Treasury::new(&opts.root, false)?;
            treasury.load_importers(&create.importers_dir)?;
            treasury.save()?;
            println!("New goods created at '{}'", opts.root)
        }
        SubCommand::Update(update) => {
            let mut treasury = Treasury::open(&opts.root)?;
            treasury.load_importers(&update.importers_dir)?;
            treasury.save()?;
            println!("Goods at '{}' updated", opts.root)
        }
        SubCommand::Store(register) => {
            let mut treasury = Treasury::open(&opts.root)?;
            let uuid = treasury.store(register.source_path, &register.importer, &[])?;
            treasury.save()?;
            println!("New asset registered as '{}'", uuid);
        }
        SubCommand::Fetch(fetch) => {
            let mut treasury = Treasury::open(&opts.root)?;
            let data = treasury.fetch(&fetch.uuid)?;
            treasury.save()?;
            println!("Asset loaded. Size: {}", data.bytes.len());

            if fetch.binary {
                let stdout = std::io::stdout();
                std::io::Write::write_all(&mut stdout.lock(), &data.bytes)?;
            } else {
                if data.bytes.len() < 1024 {
                    match std::str::from_utf8(&data.bytes) {
                        Ok(data) => {
                            println!("{}", data);
                        }
                        Err(err) => {
                            eprintln!("Data is not UTF-8. {:#}", err);
                        }
                    }
                } else {
                    eprintln!("Data is too long");
                }
            }
        }
    }

    Ok(())
}
