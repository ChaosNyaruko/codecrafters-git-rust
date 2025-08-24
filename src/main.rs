#[allow(unused_imports)]
use std::env;
#[allow(unused_imports)]
use std::fs;
use std::io::BufRead;
use std::io::Read;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use clap::Subcommand;
use flate2::read::ZlibDecoder;

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
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    }

    Ok(())
}
