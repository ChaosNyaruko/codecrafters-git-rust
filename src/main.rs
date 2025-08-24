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
            let prefix = &object[..2];
            let path = &object[2..];
            let path = PathBuf::from(".git/objects").join(prefix).join(path);
            let f = std::fs::File::open(&path).context(format!("read {:?} err", path))?;
            let z = ZlibDecoder::new(f);
            let mut reader = std::io::BufReader::new(z);
            let mut tmp = Vec::new();
            let type_n = reader.read_until(' ' as u8, &mut tmp)?;
            assert!(type_n > 0, "we must have a type");
            let size_n = reader.read_until('\0' as u8, &mut tmp)?;
            let size_vec = tmp[type_n..type_n + size_n - 1].to_owned();
            let size_str = String::from_utf8(size_vec)?;
            let size = size_str
                .parse::<u64>()
                .context(format!("num: {:?}", &size_str))?;
            tmp.resize(size as usize, 0);
            let mut content = reader.take(size);
            let content = content.read(&mut tmp)?;
            assert!(content == size as usize);
            print!("{}", String::from_utf8(tmp)?)
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

            let prefix = &blob_hash[..2];
            let path = &blob_hash[2..];
            let path = PathBuf::from(".git/objects").join(prefix).join(path);

            if *write_object {
                let f = std::fs::File::open(path)?;
                let mut e = ZlibEncoder::new(f, Compression::fast());
                e.write_all(&data).context("write object file error")?
            }
        }
    }

    Ok(())
}
