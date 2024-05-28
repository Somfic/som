use anyhow::*;
use std::{env, io::BufRead};

pub mod scanner;

fn main() -> Result<()> {
    let active_directory = std::env::current_dir().unwrap();
    let file_name = env::args().nth(1).unwrap();
    let file_path = active_directory.join(file_name);

    let file = std::fs::File::open(file_path).unwrap();
    let reader = std::io::BufReader::new(file);
    let content = reader
        .lines()
        .map(|line| line.unwrap())
        .collect::<Vec<String>>()
        .join("\n");

    Ok(())
}
