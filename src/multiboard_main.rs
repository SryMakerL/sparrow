use clap::Parser;
use sparrow::multiboard::{MultiBoardOptions, solve};
use sparrow::util::multiboard_io::{output_directory, read_input};
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;

#[derive(Parser)]
#[command(about = "Fixed-size, finite-inventory, priority-ordered multiboard nesting")]
struct Args {
    #[arg(short = 'i', long)]
    input: String,
    #[arg(short = 't', long, default_value_t = 600)]
    global_time: u64,
    #[arg(short = 's', long)]
    rng_seed: Option<u64>,
    #[arg(long)]
    workers: Option<NonZeroUsize>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let input = read_input(Path::new(&args.input))?;
    let mut options = MultiBoardOptions {
        time_limit: Duration::from_secs(args.global_time),
        workers: args.workers.map(NonZeroUsize::get),
        ..MultiBoardOptions::default()
    };
    if let Some(seed) = args.rng_seed {
        options.seed = seed;
    }
    #[cfg(feature = "live_svg")]
    {
        options.live_svg_path = Some(Path::new("data/live/.live_solution.svg").to_path_buf());
    }
    let interrupted = options.interrupted.clone();
    ctrlc::set_handler(move || interrupted.store(true, Ordering::Relaxed))?;
    let output = output_directory(Path::new("output"), &input.name);
    let summary = solve(&input, &options, &output)?;
    println!(
        "Saved {} boards to {}",
        summary.boards.len(),
        output.display()
    );
    Ok(())
}
