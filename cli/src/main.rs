use {clap::Clap, goods_treasury::*, tracing_subscriber::layer::SubscriberExt as _, uuid::Uuid};

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
    Create(CreateUpdate),
    Update(CreateUpdate),
    Store(Store),
    Fetch(Fetch),
    List(List),
    Remove(Remove),
}

/// A subcommand for creating new treasury
#[derive(Clap)]
struct CreateUpdate {
    /// Relative path to importers.
    #[clap(short, long)]
    importers: Vec<String>,
}

/// A subcommand for registering assets
#[derive(Clap)]
struct Store {
    /// Path to asset source file.
    #[clap()]
    source_path: String,

    /// Source format.
    #[clap()]
    source_format: String,

    /// Native format.
    #[clap()]
    native_format: String,

    #[clap(short, long)]
    tags: Vec<String>,
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

/// A subcommand for registering assets
#[derive(Clap)]
struct List {
    /// Filter by importer.
    #[clap(short, long)]
    native_format: Option<String>,

    /// Filter by importer.
    #[clap(short, long)]
    tags: Vec<String>,
}

/// A subcommand for registering assets
#[derive(Clap)]
struct Remove {
    /// Uuids to remove.
    #[clap(short)]
    uuids: Vec<Uuid>,
}

pub fn main() -> eyre::Result<()> {
    if let Err(err) = color_eyre::install() {
        tracing::error!("Failed to install eyre report handler: {}", err);
    }

    let cd = std::env::current_dir().unwrap();

    let opts: Opts = Opts::parse();

    let level = match opts.verbose {
        0 => tracing::Level::WARN,
        1 => tracing::Level::INFO,
        2 => tracing::Level::DEBUG,
        _ => tracing::Level::TRACE,
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
            let mut treasury = Treasury::new(cd.join(&opts.root), false)?;

            for dir_path in create.importers {
                treasury.load_importers_dir(cd.join(&dir_path))?;
            }

            treasury.save()?;

            println!("New goods created at '{}'", opts.root)
        }
        SubCommand::Update(create) => {
            let mut treasury = Treasury::open(cd.join(&opts.root))?;

            for dir_path in create.importers {
                treasury.load_importers_dir(cd.join(&dir_path))?;
            }

            treasury.save()?;

            println!("New goods created at '{}'", opts.root)
        }
        SubCommand::Store(store) => {
            let treasury = Treasury::open(cd.join(&opts.root))?;

            let uuid = treasury.store(
                store.source_path,
                &store.source_format,
                &store.native_format,
                &store.tags,
            )?;

            treasury.save()?;

            println!("New asset registered as '{}'", uuid);
        }
        SubCommand::Fetch(fetch) => {
            let mut treasury = Treasury::open(cd.join(&opts.root))?;
            let data = treasury.fetch(&fetch.uuid)?;
            println!("Asset loaded. Size: {}", data.bytes.len());

            if fetch.binary {
                let stdout = std::io::stdout();
                std::io::Write::write_all(&mut stdout.lock(), &data.bytes)?;
            } else if data.bytes.len() < 1024 {
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
        SubCommand::List(list) => {
            let treasury = Treasury::open(cd.join(&opts.root))?;
            let assets = treasury.list(&list.tags, list.native_format.as_deref());
            println!("{} assets found", assets.len());
            for asset in assets {
                if opts.verbose > 0 {
                    println!("{:#}", asset);
                } else {
                    println!("{}", asset);
                }
            }
        }
        SubCommand::Remove(remove) => {
            let treasury = Treasury::open(cd.join(&opts.root))?;
            for uuid in &remove.uuids {
                treasury.remove(*uuid);
            }
        }
    }

    Ok(())
}
