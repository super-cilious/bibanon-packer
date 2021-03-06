#[macro_use] extern crate failure;
#[macro_use] extern crate failure_derive;

extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate toml;

extern crate hsl;
extern crate image;
extern crate rusttype;
extern crate imageproc;

#[macro_use] extern crate log;
extern crate simplelog;

extern crate rpassword;

extern crate clap;
extern crate dirs;
extern crate notify;
extern crate reqwest;
extern crate pandoc;
extern crate rand;
extern crate regex;

pub use std::io::{self, Read, BufRead};
pub use std::path::{PathBuf, Path};
pub use std::sync::mpsc::channel;
pub use std::str::FromStr;
pub use std::time::Duration;
pub use std::fs;

pub use std::collections::HashMap;

use clap::{Arg, App, SubCommand, AppSettings};
use notify::{RecommendedWatcher, Watcher, RecursiveMode, DebouncedEvent};
pub use failure::{Fail, Error};
pub use regex::Regex;

pub mod api;
pub use self::api::*;

pub mod thumb;
pub use self::thumb::*;

pub type Res<T> = Result<T, Error>;

#[derive(Serialize, Deserialize)]
struct Config {
    username: String,
    password: String
}

#[derive(Serialize, Deserialize)]
pub struct Metadata {
    title: String,
    summary: String,
    source: String,
    #[serde(rename = "type")]
    type_: String,
    tags: Vec<String>,
    stats: Vec<String>,
    sub: Option<String>
}

#[derive(Serialize, Deserialize)]
pub struct Mod {
    last_mod: std::time::SystemTime
}

pub trait WithPath {
    fn with<T: AsRef<Path>>(&self, path: T) -> PathBuf;
    fn ext(&self, ext: &str) -> PathBuf;
}

impl WithPath for PathBuf {
    fn with<T: AsRef<Path>>(&self, path: T) -> PathBuf {
        let mut c = self.clone();
        c.push(path);
        c
    }

    fn ext(&self, ext: &str) -> PathBuf {
        self.with_extension(ext)
    }
}

fn section(title: &str, section: &str) -> String {
    format!("{}/{}", title, section)
}

pub const META_FILE: &str = "meta.toml";
pub const MOD_FILE: &str = "mod.toml";

fn parse_md<T: AsRef<Path>>(path: T) -> Res<(String, Vec<PathBuf>)> {
    let path = path.as_ref();
    info!("Processing {}...", path.display());

    let mut p = pandoc::new();
    p.add_input(&path).set_output(pandoc::OutputKind::Pipe)
        .add_pandoc_path_hint("C:\\Program Files\\Pandoc")
        .set_output_format(pandoc::OutputFormat::MediaWiki, vec![]);

    let out = p.execute()?;

    let content = match out {
        pandoc::PandocOutput::ToBuffer(s) => s, _ => unreachable!("AAAAAAAAAAAAAAAAAAAAAAAAAAAA PANDOKKK")
    };

    let file_regex = Regex::new(r"\[\[File:(.+?)(\|.+)*\]\]")?;
    let images: Vec<PathBuf> = file_regex.captures_iter(&content).map(|x| {
        path.with_file_name(&x[1])
    }).collect();

    Ok((content, images))
}
pub const INDEX_FILE: &str = "index.md";
pub const WATCH_WAIT: u64 = 2;

fn modded(modf: &Mod, path: &PathBuf) -> Res<bool> {
    trace!("Checking mod for {}", path.display());
    let m = fs::metadata(path)?;
    Ok(m.modified()? > modf.last_mod)
}

fn read_dir_sections(modf: &Mod, dir: &PathBuf) -> Res<(Vec<(String, String)>, Vec<PathBuf>)> {
    let mut sections = Vec::new();
    let mut images = Vec::new();

    trace!("Reading directory {}", dir.display());
    for file in fs::read_dir(&dir)? {
        let file = file?;
        let name = file.file_name();
        let name_str = name.to_string_lossy();

        let ftype = file.file_type()?;

        if !ftype.is_dir() {
            if name_str.ends_with(".md") && name != INDEX_FILE {
                let path = file.path();
                let (content, mut simages) = parse_md(&path)?;
                images.append(&mut simages);

                if modded(&modf, &path)? {
                    sections.push((name_str.trim_end_matches(".md").to_owned(), content));
                }
            }
        } else if ftype.is_dir() {
            let (sub, mut simages) = read_dir_sections(modf, &dir.with(name_str.to_string()))?;
            sub.into_iter().for_each(|(subname, v)|
                sections.push((section(&name_str.to_string(), &subname), v)));
            images.append(&mut simages);
        }
    }

    Ok((sections, images))
}

fn try_proc(cfg: &Config, client: &mut MwClient, dir: &PathBuf) -> Res<()> {
    debug!("Processing directory {}", dir.display());
    trace!("Reading meta.toml");
    let meta: Metadata = toml::from_str(&fs::read_to_string(dir.with(META_FILE))?)?;
    trace!("Reading mod.toml");
    let modf: Mod = fs::read_to_string(dir.with(MOD_FILE)).map_err(Error::from)
        .and_then(|x| Ok(toml::from_str(&x)?))
        .unwrap_or(Mod {last_mod: std::time::SystemTime::UNIX_EPOCH});

    trace!("Checking for thumbnail");
    let thumb_name = format!("{}-thumbnail.jpg", meta.title.replace(' ', "-"));
    if !Path::new(&dir.with(&thumb_name)).exists() {
        info!("Generating thumbnail... (can take a few seconds)");
        let bg = fs::read(dir.with("bg.jpg")).or_else(|_| fs::read(dir.with("bg.png"))).ok();
        fs::write(dir.with(&thumb_name), make_thumb(bg, &meta)?)?;
    }

    info!("Parsing files...");
    let index_path = dir.with(INDEX_FILE);
    let (index, mut images) = parse_md(&index_path)?;

    if modded(&modf, &index_path)? {
        info!("Uploading index...");
        client.edit_article(MwArticle { title: meta.title.clone(), text: index, summary: meta.summary.clone() })?;
    }

    let (sections, mut simages) = read_dir_sections(&modf, dir)?;
    images.append(&mut simages);

    for (name, text) in sections {
        info!("Uploading {}...", name);
        client.edit_article(MwArticle {title: section(&meta.title, &name), text, summary: meta.summary.clone()})?;
    }

    for image in images {
        if let Ok(true) = modded(&modf, &image) {
            info!("Uploading image {}...", image.display());

            let name = image.file_name().ok_or(format_err!("Invalid path for image {}!", image.display()))?;
            if image.exists() {
                client.upload(name.to_string_lossy().to_string(), image)?;
            } else {
                return Err(format_err!("Image {} doesn't exist!", image.display()).into())
            }
        }
    }

    fs::write(dir.with(MOD_FILE), toml::to_string(&Mod {last_mod: std::time::SystemTime::now()})?)?;
    info!("Packed & published!");
    Ok(())
}

fn try_watch(cfg: &Config, client: &mut MwClient, dir: &PathBuf, path: PathBuf) -> Res<()> {
    let path = path.strip_prefix(&dir)?.join(META_FILE);

    for x in path.ancestors() {
        let x_path = dir.join(x);
        if x_path.with_file_name(META_FILE).exists() {
            try_proc(cfg, client, &x_path.parent().unwrap().to_path_buf())?;
        }
    }

    Ok(())
}

fn cfg_path() -> PathBuf {
    dirs::config_dir().expect("Could not find config directory! Try a more standardized distribution.").with("bibanon_packer.toml")
}

fn set_cfg(path: PathBuf, default_username: Option<String>) -> Config {
    let sin = io::stdin();

    let cfg = {
        let mut username = String::new();
        match default_username {
            Some(x) => username = x,
            None => {
                println!("Please enter your username:");
                sin.read_line(&mut username).unwrap();
                username = username.trim().to_owned();
            }
        }

        println!("Please enter your password:");
        let password = rpassword::read_password().unwrap();

        Config { username, password }
    };

    fs::write(&path, toml::to_string(&cfg).unwrap()).expect("Error writing cfg.toml");
    cfg
}

fn get_cfg() -> Config {
    let cfg_path = cfg_path();
    if let Ok(x) = fs::read_to_string(&cfg_path) {
        toml::from_str(&x).expect("Error reading cfg.toml; Maybe delete the file to reset to a default configuration?")
    } else {
        set_cfg(cfg_path, None)
    }
}

fn main() {
    let args =
        App::new("Bibanon Packer")
            .version("0.1.0")
            .author("dreamatic#3333")
            .about("Packs stuff into dah wikeih")
            .arg(Arg::with_name("trace").short("t").help("Trace log level"))
            .subcommand(SubCommand::with_name("init")
                .about("Initialize a directory with a meta and index file.")
                .arg(Arg::with_name("DIRECTORY")
                    .index(1).help("Directory to initialize")))
            .subcommand(SubCommand::with_name("pack")
                .about("Pack a directory and upload it.")
                .arg(Arg::with_name("DIRECTORY")
                    .index(1).help("Directory to pack")))
            .subcommand(SubCommand::with_name("watch")
                .about("Watch a directory and upload it.")
                .arg(Arg::with_name("DIRECTORY")
                    .index(1).help("Directory to watch")))
            .subcommand(SubCommand::with_name("credentials")
                .about("Set credentials.")
                .arg(Arg::with_name("USERNAME")
                    .index(1).help("Username for wiki")))
            .setting(AppSettings::SubcommandRequiredElseHelp)
            .get_matches();

    let level = if args.occurrences_of("trace") > 0 { log::LevelFilter::Trace } else { log::LevelFilter::Info };
    simplelog::TermLogger::init(level, simplelog::Config::default()).unwrap();

    match args.subcommand() {
        ("init", Some(args)) => {
            let dir = fs::canonicalize(args.value_of("DIRECTORY").unwrap_or("./")).expect("Cannot parse path!");
            let _ = fs::create_dir_all(&dir);

            let default_meta = Metadata {
                title: "Something new".to_owned(),
                summary: "Something summarizing something".to_owned(),
                source: "reddit".to_owned(),
                type_: "story".to_owned(),
                tags: Vec::new(),
                stats: Vec::new(),
                sub: None
            };

            fs::write(dir.with(META_FILE), toml::to_string(&default_meta).expect("Error serializing metadata")).expect("Error writing metadata file!");
            fs::write(dir.with(INDEX_FILE), "## Description\nSomething new or something old: something incredible, waiting to be inserted.").expect("Error writing index file!");

            info!("Initialized directory!");
        },
        ("credentials", Some(args)) => {
            set_cfg(cfg_path(), args.value_of("USERNAME").map(|x| x.to_owned()));
            info!("Credentials set!");
        },
        ("pack", Some(args)) => {
            let cfg = get_cfg();

            let dir = fs::canonicalize(args.value_of("DIRECTORY").unwrap_or("./")).expect("Cannot parse path!");

            let mut client = MwClient::new().unwrap();
            client.login(cfg.username.to_owned(), cfg.password.to_owned()).unwrap();
            if let Err(x) = try_proc(&cfg, &mut client, &dir.to_owned()) {
                error!("{}", x);
            }
        },
        ("watch", Some(args)) => {
            let cfg = get_cfg();

            let dir = fs::canonicalize(args.value_of("DIRECTORY").unwrap_or("./")).expect("Cannot parse path!");

            let mut client = MwClient::new().unwrap();
            client.login(cfg.username.to_owned(), cfg.password.to_owned()).unwrap();

            let (tx, rx) = channel();
            let mut watcher: RecommendedWatcher = Watcher::new(tx, Duration::from_secs(WATCH_WAIT)).unwrap();

            watcher.watch(&dir, RecursiveMode::Recursive).unwrap();

            loop {
                match rx.recv() {
                    Ok(x) => {
                        match x {
                            DebouncedEvent::Create(path) | DebouncedEvent::Remove(path) | DebouncedEvent::Rename(_, path) => {
                                if let Err(x) = try_watch(&cfg, &mut client, &dir, path) {
                                    error!("Error updating watched folder: {}", x);
                                }
                            }, _ => ()
                        }
                    },
                    Err(x) => error!("Watch error: {}", x)
                }
            }

        }
        _ => unreachable!("AAAAAAAAAAAAAAAAA")
    }
}
