mod iyw_claw_memory_bench_cases;
mod iyw_claw_memory_bench_config;
mod iyw_claw_memory_bench_corpus;
mod iyw_claw_memory_bench_fixture;
mod iyw_claw_memory_bench_gates;
mod iyw_claw_memory_bench_metadata;
mod iyw_claw_memory_bench_process;
mod iyw_claw_memory_bench_public;
mod iyw_claw_memory_bench_quality;
mod iyw_claw_memory_bench_report;
mod iyw_claw_memory_bench_support;

#[tokio::main]
async fn main() {
    match iyw_claw_memory_bench_support::run(std::env::args().skip(1).collect()).await {
        Ok(outcome) => {
            println!("{}", outcome.message);
            if !outcome.passed {
                std::process::exit(1);
            }
        }
        Err(message) => {
            eprintln!("{message}");
            eprintln!("{}", iyw_claw_memory_bench_support::usage());
            std::process::exit(2);
        }
    }
}
