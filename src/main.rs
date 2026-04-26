use std::{collections::HashMap, fs, io::Write, path::Path};

use clap::{Parser, Subcommand, ValueEnum};
use log::{LevelFilter, ParseLevelError, info};

use layouts_rs::{
    analyzer::Analyzer,
    config::{Config, OptimizationConfig},
    corpus::Corpus,
    layout::Layout,
    metrics::Metrics,
    optimizer::{self, HillClimbOptimizer, Optimizer, SimulatedAnnealingOptimizer, Targets},
    stats::Stats,
};
use rand::{Rng, rng};

#[derive(Parser)]
struct Cli {
    #[arg(long, global = true, default_value = "info", value_parser = parse_level_filter)]
    level: LevelFilter,
    #[command(subcommand)]
    command: Command,
}

fn parse_level_filter(level: &str) -> Result<LevelFilter, ParseLevelError> {
    level.parse()
}

#[derive(Subcommand)]
enum Command {
    Analyze(AnalyzeArgs),
    Optimize(OptimizeArgs),
    GenerateOptimizationTargets(GenerateOptimizationTargetsArgs),
}

#[derive(Parser)]
struct AnalyzeArgs {
    #[command(flatten)]
    common: CommonConfig,
}

#[derive(Parser)]
struct OptimizeArgs {
    #[command(flatten)]
    common: CommonConfig,
    #[command(flatten)]
    run_options: RunOptions,
}

#[derive(Parser)]
struct GenerateOptimizationTargetsArgs {
    #[command(flatten)]
    common: CommonConfig,
    #[arg(long, value_name = "PATH", help = "Path to a TOML presets file")]
    presets: String,
}

#[derive(Parser, Clone)]
struct RunOptions {
    #[arg(
        long,
        short,
        default_value = "10",
        help = "Number of optimization iterations"
    )]
    iterations: usize,
    #[arg(long, short, help = "Random seed for optimization")]
    seed: Option<u64>,
    #[arg(
        long,
        short,
        default_value = "",
        help = "Characters to pin in their original positions during optimization"
    )]
    pinned: String,
    #[arg(
        long,
        help = "Maximum number of keys to swap in each optimization iteration"
    )]
    max_swapped: Option<usize>,
    #[arg(
        long,
        default_value = "false",
        help = "Whether to shuffle the layout before optimization"
    )]
    shuffle: bool,
    #[arg(
        long,
        short,
        value_enum,
        default_value_t = Algorithm::HillClimb,
        help = "Optimization algorithm to use"
    )]
    algorithm: Algorithm,
}

#[derive(ValueEnum, Clone)]
#[clap(rename_all = "snake_case")]
pub enum Algorithm {
    HillClimb,
    SimulatedAnnealing,
}

impl From<RunOptions> for optimizer::RunOptions {
    fn from(options: RunOptions) -> Self {
        let seed = if let Some(seed) = options.seed {
            seed
        } else {
            rng().next_u64()
        };

        Self {
            iterations: options.iterations,
            seed,
            pinned: options.pinned.chars().collect(),
            max_swapped: options.max_swapped,
            shuffle: options.shuffle,
        }
    }
}

#[derive(Parser)]
struct CommonConfig {
    #[arg(long, short)]
    config: String,
    #[arg(
        long,
        short,
        default_value = "qwerty",
        value_parser = CommonConfig::parse_layout_string,
        help = "Layout preset or custom layout string"
    )]
    layout: String,
    #[arg(
        long,
        value_name = "PATH",
        num_args = 1..,
        value_delimiter = ',',
        value_parser = CommonConfig::parse_corpus,
        help = "Paths to corpus JSON files"
    )]
    corpus: Vec<Corpus>,
}

impl CommonConfig {
    fn parse_layout_string(name: &str) -> Result<String, String> {
        let content = include_str!("../presets.yaml");
        let presets: HashMap<String, String> =
            serde_yaml::from_str(content).expect("Failed to parse presets.yaml");

        Ok(presets
            .get(name)
            .map(|s| s.to_string())
            .unwrap_or_else(|| name.to_string()))
    }

    fn parse_corpus(path: &str) -> Result<Corpus, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read corpus file {}: {e}", path))?;

        let corpus_map: HashMap<String, f64> = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse corpus file {}: {e}", path))?;

        Ok(Corpus::new(corpus_map))
    }

    fn corpus(&self) -> Corpus {
        let mut aggregated: HashMap<String, f64> = HashMap::new();

        for corpus in &self.corpus {
            for (word, count) in &corpus.word_items {
                *aggregated.entry(word.clone()).or_insert(0.0) += *count;
            }
        }

        Corpus::new(aggregated)
    }
}

impl Command {
    fn run(&self) -> anyhow::Result<()> {
        match self {
            Command::Analyze(args) => {
                let config = Config::load(Path::new(&args.common.config))?;

                let layout = Layout::new(&args.common.layout, &config.layout)
                    .map_err(|e| anyhow::anyhow!("Failed to load layout: {e}"))?;

                let corpus = args.common.corpus();
                let analyzer = Analyzer::new(corpus);

                let mut metrics = Metrics::default();
                analyzer.analyze(&layout, &mut metrics);
                let stats = Stats::from(metrics);
                let score = stats.score(&config.optimization.targets);

                info!("Layout:\n{layout}");
                info!("Optimization score: {score:.4}");
                info!("{stats}");
            }
            Command::Optimize(args) => {
                let config = Config::load(Path::new(&args.common.config))?;

                let layout = Layout::new(&args.common.layout, &config.layout)
                    .map_err(|e| anyhow::anyhow!("Failed to load layout: {e}"))?;

                info!("Initial Layout:\n{layout}");

                let corpus = args.common.corpus();
                let analyzer = Analyzer::new(corpus);
                let optimizer = Self::select_optimizer(
                    analyzer.clone(),
                    &args.run_options,
                    &config.optimization,
                );
                let optimized_layout = optimizer.optimize(&layout, args.run_options.clone().into());

                let mut metrics = Metrics::default();
                analyzer.analyze(&optimized_layout, &mut metrics);
                let stats = Stats::from(metrics);
                let score = stats.score(&config.optimization.targets);

                info!("Optimized Layout:\n{optimized_layout}");
                info!("Optimization score: {score:.4}");
                info!("{stats}");
            }
            Command::GenerateOptimizationTargets(args) => {
                let config = Config::load(Path::new(&args.common.config))?;

                let presets = Self::load_presets(Path::new(&args.presets))?;

                let corpus = args.common.corpus();
                let analyzer = Analyzer::new(corpus);

                let mut all_stats = Vec::new();
                for (name, preset) in presets {
                    let layout = Layout::new(&preset, &config.layout)
                        .map_err(|e| anyhow::anyhow!("Failed to load preset {name}: {e}"))?;

                    let mut metrics = Metrics::default();
                    analyzer.analyze(&layout, &mut metrics);
                    all_stats.push(Stats::from(metrics));
                }

                let (mean, std_dev) = Stats::mean_and_std_dev(&all_stats);
                let targets = Targets::from_mean_and_std_dev(&mean, &std_dev);

                println!("{}", targets.print(5));
            }
        }
        Ok(())
    }

    fn select_optimizer(
        analyzer: Analyzer,
        run_options: &RunOptions,
        optimization: &OptimizationConfig,
    ) -> Box<dyn Optimizer> {
        match run_options.algorithm {
            Algorithm::HillClimb => Box::new(HillClimbOptimizer::new(
                analyzer,
                optimization.targets.clone(),
            )),
            Algorithm::SimulatedAnnealing => Box::new(SimulatedAnnealingOptimizer::new(
                analyzer,
                optimization.targets.clone(),
                optimization.simulated_annealing.clone(),
            )),
        }
    }

    fn load_presets(path: &Path) -> anyhow::Result<Vec<(String, String)>> {
        let content = fs::read_to_string(path)?;
        let presets: HashMap<String, String> = serde_yaml::from_str(&content)?;
        let mut presets: Vec<_> = presets.into_iter().collect();
        presets.sort_by(|(left, _), (right, _)| left.cmp(right));
        Ok(presets)
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let env = env_logger::Env::default().default_filter_or(cli.level.as_str());
    env_logger::Builder::from_env(env)
        .format(|buf, record| {
            let style = buf.default_level_style(record.level());
            writeln!(buf, "{style}{}{style:#} {}", record.level(), record.args())
        })
        .init();

    cli.command.run()?;
    Ok(())
}
