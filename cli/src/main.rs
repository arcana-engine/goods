use {clap::Clap, goods::Goods, tracing_subscriber::layer::SubscriberExt as _, uuid::Uuid};

#[derive(Clap)]
#[clap(version = "0.1", author = "Zakarum <zakarumych@ya.ru>")]
struct Opts {
    /// Goods info file path
    #[clap(short, long, default_value = "goods.bin")]
    goods: String,

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
            let mut goods = Goods::new(&create.sources, &create.natives)?;
            goods.load_importers(&create.importers_dir)?;
            goods.save(&opts.goods)?;
            println!("New goods created at '{}'", opts.goods)
        }
        SubCommand::Update(update) => {
            let mut goods = Goods::open(&opts.goods)?;
            goods.load_importers(&update.importers_dir)?;
            goods.save(&opts.goods)?;
            println!("Goods at '{}' updated", opts.goods)
        }
        SubCommand::Store(register) => {
            let mut goods = Goods::open(&opts.goods)?;
            let uuid = goods.store(register.source_path, &register.importer, &[])?;
            goods.save(&opts.goods)?;
            println!("New asset registered as '{}'", uuid);
        }
        SubCommand::Fetch(load) => {
            let mut goods = Goods::open(&opts.goods)?;
            let data = goods.fetch(&load.uuid)?;
            goods.save(&opts.goods)?;
            println!("Asset loaded. Size: {}", data.bytes.len());
        }
    }

    Ok(())
}
