mod yamldiff;
mod keypath;
mod error;
mod strategy;
#[macro_use]
extern crate error_chain;

use clap::Parser;
use yamldiff::{Opts,do_diff};
use error_chain::ChainedError;
use std::process::exit;


fn main() {
    let opts: Opts = Opts::parse();
    let result = do_diff(&opts);
    match result {
        Ok(n) => exit(n),
        Err(e) => {
            eprintln!("yamldiff: {}",e.display_chain().to_string());
            exit(2)
        }
    }
}

