//! Measure parse throughput over a directory of R files.
//!
//! Usage: taskset -c 2 cargo run --release -p air_r_parser --example parse_bench -- <dir>
//!
//! Reports best-of-N wall time (the machine's hybrid cores make single runs
//! noisy, so pin a core and take the minimum). For reference, at the time
//! tree-sitter was removed it measured 3.6 MB/s where this parser measured
//! 23.3 MB/s (6.4x) over 41.3 MB of R sources.

use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;

use air_r_parser::RParserOptions;
use air_r_parser::parse;

const ITERATIONS: usize = 5;

fn collect_r_files(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            if name != "target" && name != ".git" {
                collect_r_files(&path, files);
            }
        } else if path
            .extension()
            .is_some_and(|ext| ext == "R" || ext == "r")
        {
            files.push(path);
        }
    }
}

fn main() {
    let root = std::env::args().nth(1).expect("usage: parse_bench <dir>");
    let mut files = Vec::new();
    collect_r_files(Path::new(&root), &mut files);

    let sources: Vec<String> = files
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .collect();
    let bytes: usize = sources.iter().map(String::len).sum();
    println!(
        "{} files, {:.1} MB, best of {ITERATIONS} runs:",
        sources.len(),
        bytes as f64 / 1e6
    );

    let mut best = Duration::MAX;
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        for source in &sources {
            std::hint::black_box(parse(source, RParserOptions::default()));
        }
        best = best.min(start.elapsed());
    }

    println!(
        "parse: {best:>10.2?}  ({:.1} MB/s)",
        bytes as f64 / 1e6 / best.as_secs_f64()
    );
}
