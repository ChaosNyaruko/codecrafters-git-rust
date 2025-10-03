#[allow(unused_imports)]
use sha1::{Digest, Sha1};
use std::collections::{self, HashMap};
use std::fs::{self};
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
        #[arg(short = 'w')]
        write_object: bool,

        filename: String,
    },
    LsTree {
        #[arg(long = "name-only")]
        name_only: bool,

        object: String,
    },
    WriteTree,
    CommitTree {
        tree_object: String,

        #[arg(short = 'm')]
        message: String,

        #[arg(short = 'p')]
        parent: String,
    },
    Clone {
        git_url: String,
        dir: String,
    },
}

#[repr(u8)]
enum PackObjType {
    Commit = 1,
    Tree,
    Blob,
    Tag,
    OfsDelta,
    RefDelta,
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

#[derive(Default)]
struct BaseRef {
    content: Vec<u8>,
    otype: u8,
}

impl BaseRef {
    fn new(content: &[u8], otype: u8) -> Self {
        BaseRef {
            content: content.to_vec(),
            otype,
        }
    }
}

fn decode_size(buf: &[u8], i: &mut usize, offset_mode: bool) -> usize {
    let mut size = buf[*i] as usize & (if !offset_mode { 0x0f } else { 0x7f });
    let mut shift = 4;
    if offset_mode {
        shift = 7;
    }
    while buf[*i] & 0x80 != 0 {
        let b = buf[*i + 1] as usize;
        size |= (b & 0x7F) << shift;
        *i += 1;
        shift += 7;
    }
    *i += 1;
    size
}

fn store_idx(idx: &mut HashMap<String, BaseRef>, otype: u8, size: usize, data: &mut Vec<u8>) {
    let header = match otype {
        1 => "commit ",
        2 => "tree ",
        3 => "blob ",
        _ => {
            unimplemented!("we don't know how to deal with other types: {}", otype);
        }
    };
    let mut obj = header.as_bytes().to_vec();
    let mut size = Vec::from(size.to_string());
    obj.append(&mut size);
    obj.push(b'\0');
    obj.extend_from_slice(&data);
    let mut hasher = Sha1::new();
    hasher.update(&obj);
    let obj_hash = hasher.finalize();
    let obj_hash = format!("{:x}", obj_hash);
    eprintln!("{obj_hash}");

    idx.insert(obj_hash, BaseRef::new(&data, otype));
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
        Commands::CommitTree {
            tree_object,
            message,
            parent,
        } => {
            use chrono::Local;
            use std::fmt::Write;

            let now = Local::now();
            let now = now.timestamp();
            let mut commit = Vec::new();
            writeln!(commit, "tree {}", tree_object)?;
            writeln!(commit, "parent {}", parent,)?;
            writeln!(
                commit,
                "author {} <cabbageparadise@gmail.com> {} {}",
                "chaosnyaruko", now, "+0800",
            )?;
            writeln!(
                commit,
                "committer {} <cabbageparadise@gmail.com> {} {}",
                "chaosnyaruko", now, "+0800",
            )?;
            writeln!(commit, "\n{message}")?;

            let mut data = b"commit ".to_vec();
            let mut size = Vec::from(commit.len().to_string());
            data.append(&mut size);
            data.push(b'\0');
            data.append(&mut commit);
            let mut hasher = Sha1::new();
            hasher.update(&data);
            let commit_hash = hasher.finalize();
            let commit_hash = format!("{:x}", commit_hash);
            write_object(&commit_hash, &data)?;
            println!("{}", commit_hash);
        }
        Commands::Clone { git_url, dir } => {
            let info_git_url = git_url.to_owned() + "/info/refs?service=git-upload-pack";
            if false {
                // TODO: rewrite it in an "await" way
                let mut resp = reqwest::blocking::get(&info_git_url)?;

                let status = resp.status();
                assert!(status == 200 || status == 304);
                if status == 304 {
                    anyhow::bail!("not got a valid service response");
                }

                let mut body = Vec::new();
                resp.copy_to(&mut body)?;
                let mut offset = 0;

                let mut head = String::new();
                while offset < body.len() {
                    let line = read_pkt_line(&body, &mut offset)?;
                    eprint!("{}", str::from_utf8(line)?);
                    let mut s = line.split(|c| *c == b' ' || *c == b'\0');
                    if let Some(h) = s.next() {
                        if let Some(pointer) = s.next() {
                            let pointer = str::from_utf8(pointer)?;
                            if pointer == "HEAD" {
                                head = String::from_utf8(h.to_vec())?;
                                break;
                            }
                        }
                    }
                }
                let head = "42ef8cfdd14525539c47310fa2d83bcfe73b7ee4";
                eprintln!("{head}");
                assert_eq!(head.len(), 40);
                let pack_git_url = git_url.to_owned() + "/git-upload-pack";
                let want = format!("want {head}\n");
                let want = create_pkt_line(want.as_bytes());
                let flush = create_pkt_line(b"");
                let done = create_pkt_line(b"done\n");

                let mut body = Vec::with_capacity(want.len() + flush.len() + done.len());
                body.extend_from_slice(&want);
                body.extend_from_slice(&flush);
                body.extend_from_slice(&done);

                use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
                let mut headers = HeaderMap::new();
                headers.insert(
                    CONTENT_TYPE,
                    HeaderValue::from_static("application/x-git-upload-pack-request"),
                );
                let client = reqwest::blocking::Client::new();
                let mut resp = client
                    .post(&pack_git_url)
                    .headers(headers)
                    .body(body.clone())
                    .send()?;
                eprintln!("{pack_git_url}, body{:?}, {}", &body, resp.status());

                resp.copy_to(&mut std::io::stdout())
                    .context("write to stdout")?;
            }
            let mut offset = 0;
            let ori_buf = std::fs::read("server2.log").context("read packfile err")?;
            let nak = read_pkt_line(&ori_buf, &mut offset)?;
            eprintln!("{}", str::from_utf8(nak)?);

            let sig = &ori_buf[offset..offset + 4];
            eprintln!("{sig:?}");
            let version = &ori_buf[offset + 4..offset + 8];
            eprintln!("{version:?}");
            let object_num = &ori_buf[offset + 8..offset + 12];
            let object_num = u32::from_be_bytes(object_num.try_into()?);
            eprintln!("{object_num:?}");
            offset += 12;

            let mut buf = &ori_buf[offset..];

            let mut idx = collections::HashMap::<String, BaseRef>::new();
            for k in 0..object_num {
                let mut i = 0;
                let otype = (buf[i] >> 4) & 0x07;
                let size = decode_size(&buf, &mut i, false);
                eprintln!("k:{k} type: {}, size: {}", otype, size);
                match otype {
                    1 | 2 | 3 | 4 => {
                        let mut z = ZlibDecoder::new(&buf[i..]);
                        let mut data = Vec::new();
                        let read_size = z
                            .read_to_end(&mut data)
                            .context("decompress a normal object")?;
                        let inb = z.total_in();
                        let out = z.total_out();
                        assert_eq!(data.len(), size);
                        assert_eq!(read_size, size);
                        assert_eq!(read_size, out as usize);
                        buf = &buf[i + inb as usize..];

                        store_idx(&mut idx, otype, size, &mut data);
                    }
                    7 => {
                        let base_ref = hex::encode(&buf[i..i + 20]);
                        let base = idx.get(&base_ref);
                        if base.is_none() {
                            anyhow::bail!("base {} not found", base_ref);
                        }
                        let base = base.unwrap();
                        i += 20;
                        let mut z = ZlibDecoder::new(&buf[i..]);
                        let mut data = Vec::new();
                        z.read_to_end(&mut data).context("decompress ref delta")?;
                        let inb = z.total_in();
                        assert_eq!(data.len(), size);
                        let mut j = 0;
                        let src_size = decode_size(&data, &mut j, true);
                        let dst_size = decode_size(&data, &mut j, true);
                        let mut new_dst = Vec::<u8>::with_capacity(dst_size);
                        eprintln!("xx {src_size}, {dst_size}");
                        while j < data.len() {
                            let ins = if data[j] & 0x80 != 0 { "COPY" } else { "ADD" };
                            if ins == "COPY" {
                                let size_to_copy = (data[j] >> 4) & 0b0111;
                                let s1 = size_to_copy & 0b001 != 0;
                                let s2 = size_to_copy & 0b010 != 0;
                                let s3 = size_to_copy & 0b100 != 0;
                                let offset_to_copy = (data[j]) & 0b1111;
                                let of1 = offset_to_copy & 0b0001 != 0;
                                let of2 = offset_to_copy & 0b0010 != 0;
                                let of3 = offset_to_copy & 0b0100 != 0;
                                let of4 = offset_to_copy & 0b1000 != 0;
                                j += 1;
                                let mut start: usize = 0;
                                if of1 {
                                    start |= (data[j]) as usize;
                                    j += 1;
                                }
                                if of2 {
                                    start |= (data[j] as usize) << 8;
                                    j += 1;
                                }
                                if of3 {
                                    start |= (data[j] as usize) << 16;
                                    j += 1;
                                }
                                if of4 {
                                    start |= (data[j] as usize) << 24;
                                    j += 1;
                                }

                                let mut size: usize = 0;
                                if s1 {
                                    size |= data[j] as usize;
                                    j += 1;
                                }
                                if s2 {
                                    size |= (data[j] as usize) << 8;
                                    j += 1;
                                }
                                if s3 {
                                    size |= (data[j] as usize) << 16;
                                    j += 1;
                                }
                                eprintln!("start:{}, size:{}", start, size);

                                new_dst.extend_from_slice(&base.content[start..start + size]);
                            } else {
                                // if ins == "ADD"
                                let add_size: usize = (data[j] as usize) & 0x7F;
                                j += 1;
                                let added = &data[j..j + add_size];
                                j += add_size;
                                new_dst.extend_from_slice(added);
                            }
                        }
                        assert_eq!(j, data.len());
                        assert_eq!(new_dst.len(), dst_size);
                        // TODO: why can't i wrap it in "store_idx"
                        // store_idx(&mut idx, base.otype, dst_size, &mut new_dst);
                        let header = match base.otype {
                            1 => "commit ",
                            2 => "tree ",
                            3 => "blob ",
                            _ => {
                                unimplemented!(
                                    "we don't know how to deal with other types: {}",
                                    base.otype
                                );
                            }
                        };
                        let mut obj = header.as_bytes().to_vec();
                        let size = Vec::from(dst_size.to_string());
                        obj.extend_from_slice(&size);
                        obj.push(b'\0');
                        obj.extend_from_slice(&new_dst);
                        let mut hasher = Sha1::new();
                        hasher.update(&obj);
                        let obj_hash = hasher.finalize();
                        let obj_hash = format!("{:x}", obj_hash);
                        eprintln!("{obj_hash}");

                        idx.insert(obj_hash, BaseRef::new(&new_dst, base.otype));
                        buf = &buf[i + inb as usize..];
                    }
                    6 => {
                        unimplemented!("{}", otype);
                    }
                    unknown => {
                        unreachable!("unknown object type {unknown}")
                    }
                }
            }
        }
    }

    Ok(())
}

fn create_pkt_line(s: &[u8]) -> Vec<u8> {
    let len = if s.len() == 0 { 0 } else { s.len() + 4 };
    let len = format!("{len:04x}");
    eprintln!("{len}");
    let mut res = len.bytes().collect::<Vec<u8>>();
    res.extend(s);
    return res;
}

fn read_pkt_line<'a>(buf: &'a [u8], offset: &mut usize) -> Result<&'a [u8], anyhow::Error> {
    let len = &buf[*offset..*offset + 4];
    *offset += 4;
    let len = usize::from_str_radix(str::from_utf8(len)?, 16)?;
    if len == 0 {
        // for "0000"
        return Ok(b"");
    }
    let res = &buf[*offset..*offset + len - 4];
    *offset += len - 4;
    Ok(res)
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
