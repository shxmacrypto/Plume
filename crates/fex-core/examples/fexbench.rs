//! Mesures de vitesse du cœur de Plume FEx sur de vrais dossiers :
//! `cargo run --release -p fex-core --example fexbench -- list C:\Windows\System32`
//! `cargo run --release -p fex-core --example fexbench -- index C:\ -- main "*.rs" "ext:png size:>1mb"`

use fex_core::{
    SortKey,
    index::{self, Progress},
    list_dir,
    query::{Clock, Order, Query},
    sort_entries,
};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("list") => {
            for p in &args[1..] {
                bench_list(Path::new(p));
            }
        }
        Some("index") => bench_index(&args[1..]),
        _ => eprintln!("usage : fexbench list <dossier>... | fexbench index <racine>... [-- <recherche>...]"),
    }
}

fn bench_list(p: &Path) {
    // premier passage : cache disque froid ou chaud selon l'historique ; les suivants montrent le coût réel de l'API
    let mut best = f64::MAX;
    let mut n = 0;
    for _ in 0..5 {
        let t = Instant::now();
        match list_dir(p, None) {
            Ok(l) => {
                n = l.len();
                best = best.min(t.elapsed().as_secs_f64() * 1000.0);
            }
            Err(e) => return println!("{} : {e}", p.display()),
        }
    }
    let mut l = list_dir(p, None).unwrap();
    let t = Instant::now();
    sort_entries(&mut l, SortKey::Name, true, true);
    let sort = t.elapsed().as_secs_f64() * 1000.0;
    println!("{} : {n} entrées, lecture {best:.2} ms, tri par nom {sort:.2} ms", p.display());
}

/// Construit l'index des racines données, puis chronomètre quelques recherches.
fn bench_index(args: &[String]) {
    let split = args.iter().position(|a| a == "--").unwrap_or(args.len());
    let roots: Vec<PathBuf> = args[..split].iter().map(PathBuf::from).collect();
    let queries: Vec<&str> = args.get(split + 1..).unwrap_or(&[]).iter().map(String::as_str).collect();
    let threads = std::env::var("FEX_THREADS").ok().and_then(|t| t.parse().ok()).unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get()));
    let (progress, cancel) = (Progress::default(), AtomicBool::new(false));
    let t = Instant::now();
    let mut ix = index::build(&roots, threads, &progress, &cancel);
    let walk = t.elapsed().as_secs_f64();
    let t = Instant::now();
    ix.compute_rank(threads);
    let rank = t.elapsed().as_secs_f64();
    let dirs = progress.folders.load(Ordering::Relaxed);
    println!(
        "{} éléments ({dirs} dossiers lus) sur {threads} fils : parcours {walk:.2} s, ordre alphabétique {rank:.2} s, {:.0} Mo en mémoire, noms {:.0} Mo",
        ix.len(),
        ix.bytes() as f64 / 1048576.0,
        ix.names.len() as f64 / 1048576.0
    );
    let clock = Clock::system();
    for q in queries {
        let query = Query::parse(q, &clock);
        let mut best = f64::MAX;
        let mut found = 0;
        for _ in 0..5 {
            let t = Instant::now();
            found = ix.search(&query, None, Order::Name, false, 10_000, threads, &cancel).total;
            best = best.min(t.elapsed().as_secs_f64() * 1000.0);
        }
        let first: Vec<String> =
            ix.search(&query, None, Order::Name, false, 3, threads, &cancel).ids.iter().map(|&i| ix.path(i).display().to_string()).collect();
        println!("  {q:<28} {found:>9} résultats en {best:>7.2} ms   {}", first.join(" | "));
    }
}
