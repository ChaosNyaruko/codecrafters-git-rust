#[allow(unused_imports)]
use std::env;
#[allow(unused_imports)]
use std::fs;
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
            let obj = std::fs::read(&path).context(format!("read {:?} err", path))?;
            let mut z = ZlibDecoder::new(&obj[..]);
            let mut s = String::new();
            z.read_to_string(&mut s)?;
            let s: Vec<_> = s.splitn(3, |c| c == ' ' || c == '\0').collect();
            let size = s[1].parse::<usize>()?;
            assert_eq!(s[2].len(), size);
            print!("{}", s[2])
        }
    }

    Ok(())
}
