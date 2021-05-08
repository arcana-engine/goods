use {clap::Clap, reliquary::Reliquary, tracing_subscriber::layer::SubscriberExt as _, uuid::Uuid};

#[derive(Clap)]
#[clap(version = "0.1", author = "Zakarum <zakarumych@ya.ru>")]
struct Opts {
    /// Reliquary info file path
    #[clap(short, long, default_value = "reliquary.bin")]
    reliquary: String,

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
    #[clap(short, long)]
    sources: String,

    #[clap(short, long)]
    natives: String,

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
            let mut reliquary = Reliquary::new(&create.sources, &create.natives)?;
            reliquary.load_importers(&create.importers_dir)?;
            reliquary.save(&opts.reliquary)?;
            println!("New reliquary created at '{}'", opts.reliquary)
        }
        SubCommand::Update(update) => {
            let mut reliquary = Reliquary::open(&opts.reliquary)?;
            reliquary.load_importers(&update.importers_dir)?;
            reliquary.save(&opts.reliquary)?;
            println!("Reliquary at '{}' updated", opts.reliquary)
        }
        SubCommand::Store(register) => {
            let mut reliquary = Reliquary::open(&opts.reliquary)?;
            let uuid = reliquary.store(register.source_path, &register.importer, &[])?;
            reliquary.save(&opts.reliquary)?;
            println!("New relic registered as '{}'", uuid);
        }
        SubCommand::Fetch(load) => {
            let mut reliquary = Reliquary::open(&opts.reliquary)?;
            let data = reliquary.fetch(load.uuid)?;
            reliquary.save(&opts.reliquary)?;
            println!("Relic loaded. Size: {}", data.len());
        }
    }

    // let reliquary = Reliquary::open("reliquary.info").unwrap();

    Ok(())
}
