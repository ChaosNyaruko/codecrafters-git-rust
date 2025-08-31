use sha1::{Digest, Sha1};
#[allow(unused_imports)]
use std::env;
use std::fs::{self, DirEntry};
use std::{
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
};

use anyhow::Context;
use clap::Parser;
use clap::Subcommand;
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};

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
    WriteTree,
}

#[derive(Debug)]
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
            let hash = calc_blob_hash(Path::new(filename), *write_object)?;
            println!("{}", hash);
        }
        Commands::LsTree { name_only, object } => {
            let obj = GitObject::new(object)?;
            obj.cat(*name_only)?;
        }
        Commands::WriteTree => {
            // SKIP: read all files/directories(recursively) where .git exists, now we just assume
            // the command must be executed at where .git exactly exists.
            //
            // sort the entries
            //
            // calc hashes and write to the object file.
            //
            let hash = dir_hash(Path::new("."), true, true)?;
            println!("{hash}")
        }
    }

    Ok(())
}

struct Objects(Vec<Object>);
// TODO: combine it with GitObject
#[derive(Debug)]
struct Object {
    kind: ObjectType,
    size: usize,
    hash: String,
    path: PathBuf,
    mode: &'static str,
}

impl std::fmt::Display for Objects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for item in &self.0 {
            write!(f, "{}\n", item)?; // uses MyStruct::fmt
        }
        write!(f, "")
    }
}

impl std::fmt::Display for Object {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:0>6} {} {:?} {}",
            self.mode,
            self.kind,
            self.path.file_name().unwrap(),
            self.hash
        )
    }
}

fn dir_hash(dir: &Path, print: bool, write: bool) -> Result<String, anyhow::Error> {
    let mut objs = Objects(Vec::new());
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let mut obj = Object {
                size: 0,
                hash: String::new(),
                path: entry.path(),
                kind: ObjectType::Blob,
                mode: "000000",
            };
            let path = entry.path();
            // ignore the ".git" directory
            if path.is_dir()
                && path.file_name().unwrap().to_str().unwrap().cmp(".git")
                    == std::cmp::Ordering::Equal
            {
                continue;
            }
            if path.is_dir() {
                obj.hash = dir_hash(&path, false, true)?;
                obj.kind = ObjectType::Tree;
            } else {
                obj.kind = ObjectType::Blob;
                obj.hash = calc_blob_hash(&path, true)?;
            }
            objs.0.push(obj);
        }
    }
    objs.0.sort_by(|p1, p2| {
        let oa = p1.path.file_name().expect("oa should be Some");
        let ob = p2.path.file_name().expect("ob should be Some");
        oa.cmp(ob)
    });

    let mut buf = Vec::new();
    for obj in &mut objs.0 {
        let mode = if obj.path.is_dir() {
            "40000"
        } else if obj.path.is_symlink() {
            "120000"
        } else if obj.path.is_file() {
            "100644"
        } else {
            // TODO: executable
            "100755"
        };
        obj.mode = mode;
        buf.extend_from_slice(obj.mode.as_bytes());
        buf.extend_from_slice(" ".as_bytes());
        buf.extend_from_slice(
            obj.path
                .file_name()
                .expect("write file name")
                .to_str()
                .expect("osstr to str")
                .as_bytes(),
        );
        buf.extend_from_slice("\0".as_bytes());
        buf.extend_from_slice(hex::decode(obj.hash.clone()).unwrap().as_slice());
    }
    if print {
        eprintln!("buf len {:?}/{}\n{}", dir, buf.len(), objs);
    }

    let mut data = vec![b't', b'r', b'e', b'e', b' '];
    let mut size = Vec::from(buf.len().to_string());
    data.append(&mut size);
    data.push(b'\0');
    data.append(&mut buf);
    let mut hasher = Sha1::new();
    hasher.update(&data);
    let tree_hash = hasher.finalize();
    let tree_hash = format!("{:x}", tree_hash);
    if write {
        write_object(&tree_hash, &data).context("write to tree object")?;
    }
    Ok(tree_hash)
}

fn calc_blob_hash(filename: &Path, write: bool) -> Result<String, anyhow::Error> {
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
    if write {
        write_object(&blob_hash, &data).context("write to blob object")?;
    }
    Ok(blob_hash)
}

fn write_object(hash: &String, data: &[u8]) -> Result<(), anyhow::Error> {
    let prefix = &hash[..2];
    let path = &hash[2..];
    let path = PathBuf::from(".git/objects").join(prefix).join(path);
    let f = if std::fs::exists(&path)? {
        std::fs::File::open(&path).context(format!("open file {:?}", path))?
    } else {
        let prefix = PathBuf::from(".git/objects").join(prefix);
        std::fs::create_dir_all(&prefix)?;
        std::fs::File::create(&path).context(format!("create file {:?}", path))?
    };
    let mut e = ZlibEncoder::new(f, Compression::fast());
    e.write_all(&data).context("write object file error")
}
