#[allow(unused_imports)]
use std::env;
#[allow(unused_imports)]
use std::fs;
use std::io::BufRead;
use std::io::Read;
use std::io::Write;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use clap::Subcommand;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Init a git repo
    Init,
    /// Cat-File
    CatFile {
        #[arg(short = 'p', group = "type")]
        pretty_print: bool, // TODO:  exclusive with -e -t -s

        object: String,
    },
    HashObject {
        #[arg(short)]
        write_object: bool,

        filename: String,
    },
    LsTree {
        #[arg(long = "name-only")]
        name_only: bool,

        object: String,
    },
}

enum ObjectType {
    Blob,
    Tree,
    Commit,
}

impl std::fmt::Display for ObjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ObjectType::Blob => "blob",
            ObjectType::Tree => "tree",
            ObjectType::Commit => "commit",
        };
        write!(f, "{}", s)
    }
}

struct GitObject {
    _size: usize,
    kind: ObjectType,
    content: Vec<u8>,
}

impl GitObject {
    fn new(object: &String) -> Result<Self, anyhow::Error> {
        let prefix = &object[..2];
        let path = &object[2..];
        let path = PathBuf::from(".git/objects").join(prefix).join(path);
        let f = std::fs::File::open(&path).context(format!("read {:?} err", path))?;
        let z = ZlibDecoder::new(f);
        let mut reader = std::io::BufReader::new(z);
        let mut buf = Vec::new();
        let type_n = reader.read_until(' ' as u8, &mut buf)?;
        assert!(type_n > 0, "we must have a type");
        let kind = str::from_utf8(&buf).context("parse object type")?;
        let kind = match kind {
            "blob " => ObjectType::Blob,
            "tree " => ObjectType::Tree,
            "commit " => ObjectType::Commit,
            _ => unreachable!("unsupport object type {}", kind),
        };
        buf.clear();
        reader.read_until('\0' as u8, &mut buf)?;
        let size_str = str::from_utf8(&buf[..buf.len() - 1]).context("convert size")?;
        let size = size_str
            .parse::<usize>()
            .context(format!("num: {:?}", &size_str))?;
        buf.clear();
        let mut reader = reader.take(size as u64);
        let content_len = reader.read_to_end(&mut buf)?;
        assert_eq!(content_len, size as usize, "{object}");
        Ok(GitObject {
            _size: size,
            kind,
            content: buf,
        })
    }

    fn cat(&self, name_only: bool) -> Result<(), anyhow::Error> {
        match self.kind {
            ObjectType::Blob => {
                print!("{}", str::from_utf8(&self.content)?)
            }
            ObjectType::Tree => {
                // TODO: perf
                let mut i = 0;
                loop {
                    let mut mode = Vec::with_capacity(6);
                    while i < self.content.len() {
                        let c = self.content[i];
                        i += 1;
                        if c == b' ' {
                            break;
                        }
                        mode.push(c);
                    }
                    let mode = format!("{:0>6}", str::from_utf8(&mode)?);

                    let mut name = Vec::new();
                    while i < self.content.len() {
                        let c = self.content[i];
                        i += 1;
                        if c == b'\0' {
                            break;
                        }
                        name.push(c);
                    }
                    let name = str::from_utf8(&name)?;

                    let hash = &self.content[i..i + 20];
                    let hash = hex::encode(hash);
                    i += 20;
                    let item = GitObject::new(&hash).context(format!("{name}, {hash}"))?;
                    if !name_only {
                        println!("{} {} {}\t{}", mode, item.kind, hash, name);
                    } else {
                        println!("{}", name);
                    }
                    if i >= self.content.len() {
                        break;
                    }
                }
            }
            ObjectType::Commit => unimplemented!("commit cannot be printed"),
        }
        Ok(())
    }
}

fn main() -> Result<(), anyhow::Error> {
    // You can use print statements as follows for debugging, they'll be visible when running tests.
    eprintln!("Logs from your program will appear here!");

    let cli = Cli::parse();
    match &cli.command {
        Commands::Init => {
            fs::create_dir(".git")?;
            fs::create_dir(".git/objects")?;
            fs::create_dir(".git/refs")?;
            fs::write(".git/HEAD", "ref: refs/heads/main\n")?;
            println!("Initialized git directory")
        }
        Commands::CatFile {
            pretty_print,
            object,
        } => {
            // use std::env;
            // println!("cwd = {}", env::current_dir()?.display());
            if !*pretty_print {
                anyhow::bail!("we only support pretty_print (-p) now");
            }
            let obj = GitObject::new(object)?;
            obj.cat(false)?
        }
        Commands::HashObject {
            write_object,
            filename,
        } => {
            use sha1::{Digest, Sha1};

            let mut hasher = Sha1::new();
            let mut file = std::fs::read(filename).context("read file err")?;
            let mut size = Vec::from(file.len().to_string());
            let mut data = vec![b'b', b'l', b'o', b'b', b' '];
            data.append(&mut size);
            data.push(b'\0');
            data.append(&mut file);
            hasher.update(&data);
            let blob_hash = hasher.finalize();
            let blob_hash = format!("{:x}", blob_hash);
            println!("{}", blob_hash);

            if *write_object {
                let prefix = &blob_hash[..2];
                let path = &blob_hash[2..];
                let path = PathBuf::from(".git/objects").join(prefix).join(path);
                let f = if std::fs::exists(&path)? {
                    std::fs::File::open(&path).context(format!("open file {:?}", path))?
                } else {
                    let prefix = PathBuf::from(".git/objects").join(prefix);
                    std::fs::create_dir_all(&prefix)?;
                    std::fs::File::create(&path).context(format!("create file {:?}", path))?
                };
                let mut e = ZlibEncoder::new(f, Compression::fast());
                e.write_all(&data).context("write object file error")?
            }
        }
        Commands::LsTree { name_only, object } => {
            let obj = GitObject::new(object)?;
            obj.cat(*name_only)?;
        }
    }

    Ok(())
}
