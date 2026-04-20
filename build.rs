use std::fs::{File, create_dir_all, read_to_string};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use askama::Template;

use dsl::{Rules, parse};

#[derive(Debug, Template)]
#[template(path = "metrics.rs.j2", escape = "none")]
struct MetricsTemplate {
    metrics: Vec<dsl::Metric>,
}

#[derive(Debug, Template)]
#[template(path = "stats.rs.j2", escape = "none")]
struct StatsTemplate {
    stats: dsl::Stats,
    metrics: Vec<dsl::Metric>,
}

#[derive(Debug, Template)]
#[template(path = "targets.rs.j2", escape = "none")]
struct TargetsTemplate {
    targets: Vec<dsl::Target>,
}

#[derive(Debug, Template)]
#[template(path = "optimization.rs.j2", escape = "none")]
struct OptimizationTemplate {
    stats: dsl::Stats,
    targets: Vec<dsl::Target>,
}

fn main() -> Result<()> {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").context("read CARGO_MANIFEST_DIR")?);
    let rules_path = manifest_dir.join("rules.kdl");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").context("read OUT_DIR")?);

    let rules: Rules = parse(&read_to_string(&rules_path)?)?;

    println!("cargo:rerun-if-changed={}", rules_path.display());

    generate(
        &rules_path.to_string_lossy(),
        "templates/metrics.rs.j2",
        &out_dir.join("metrics.rs"),
        MetricsTemplate {
            metrics: rules.metrics.clone(),
        },
    )?;

    generate(
        &rules_path.to_string_lossy(),
        "templates/stats.rs.j2",
        &out_dir.join("stats.rs"),
        StatsTemplate {
            stats: rules.stats.clone(),
            metrics: rules.metrics.clone(),
        },
    )?;

    generate(
        &rules_path.to_string_lossy(),
        "templates/targets.rs.j2",
        &out_dir.join("targets.rs"),
        TargetsTemplate {
            targets: rules.targets.clone(),
        },
    )?;

    generate(
        &rules_path.to_string_lossy(),
        "templates/optimization.rs.j2",
        &out_dir.join("optimization.rs"),
        OptimizationTemplate {
            stats: rules.stats.clone(),
            targets: rules.targets.clone(),
        },
    )?;

    Ok(())
}

fn generate(
    config_path: &str,
    template_path: &str,
    out_file: &std::path::Path,
    template: impl askama::Template,
) -> Result<()> {
    println!("cargo:rerun-if-changed={config_path}");
    println!("cargo:rerun-if-changed={template_path}");

    let rendered = template
        .render()
        .with_context(|| format!("failed to render {template_path}"))?;

    create_dir_all(out_file.parent().unwrap())?;
    File::create(out_file)?.write_all(rendered.as_bytes())?;

    Command::new("rustfmt")
        .arg("--edition")
        .arg("2024")
        .arg(out_file)
        .status()?;

    Ok(())
}

mod filters {
    use dsl::{Expression, RustExpression, Stat, Ty};

    #[askama::filter_fn]
    pub fn print_ty(ty: &Ty, _: &dyn askama::Values) -> askama::Result<String> {
        Ok(match ty {
            Ty::Scalar => "f64".to_string(),
            Ty::Map(kind) => format!("IndexMap<{}, f64>", kind.0),
        })
    }

    #[askama::filter_fn]
    pub fn print_target_ty(ty: &Ty, _: &dyn askama::Values) -> askama::Result<String> {
        Ok(match ty {
            Ty::Scalar => "SingleTarget".to_string(),
            Ty::Map(kind) => format!("MapTarget<{}>", kind.0),
        })
    }

    #[askama::filter_fn]
    pub fn print_rust(expr: &RustExpression, _: &dyn askama::Values) -> askama::Result<String> {
        Ok(expr.0.clone())
    }

    #[askama::filter_fn]
    pub fn print_rust_opt(
        expr: &Option<RustExpression>,
        _: &dyn askama::Values,
    ) -> askama::Result<Option<String>> {
        Ok(expr.as_ref().map(|e| e.0.clone()))
    }

    #[askama::filter_fn]
    pub fn print_expr(expr: &Expression, _: &dyn askama::Values) -> askama::Result<String> {
        Ok(expr_to_rust(expr))
    }

    #[askama::filter_fn]
    pub fn print_stat_value(stat: &Stat, _: &dyn askama::Values) -> askama::Result<String> {
        let name = &stat.name;
        match &stat.value {
            Expression::Ref(reference) if reference == name => Ok(format!("{name},")),
            value => Ok(format!("{name}: {},", expr_to_rust(value))),
        }
    }

    fn expr_to_rust(expr: &Expression) -> String {
        match expr {
            Expression::Ref(name) => name.clone(),
            Expression::Sum {
                expression,
                condition,
            } => {
                if let Some(condition) = condition {
                    format!(
                        "map_sum({}, |group| {})",
                        expr_to_rust_ref(expression),
                        condition.0
                    )
                } else {
                    format!("map_sum({}, |_| true)", expr_to_rust_ref(expression))
                }
            }
            Expression::Add(e1, e2) => format!("({} + {})", expr_to_rust(e1), expr_to_rust(e2)),
            Expression::Percent(e1, e2) => {
                format!("percentage({}, {})", expr_to_rust(e1), expr_to_rust(e2))
            }
            Expression::Ratio(e1, e2) => {
                format!("ratio({}, {})", expr_to_rust(e1), expr_to_rust(e2))
            }
            Expression::Normalize { map, total } => format!(
                "normalize({}, {})",
                expr_to_rust_ref(map),
                expr_to_rust(total)
            ),
            Expression::TopN { map, n } => {
                format!("topn(({}), {n})", expr_to_rust_ref(map))
            }
        }
    }

    fn expr_to_rust_ref(expr: &Expression) -> String {
        format!("&({})", expr_to_rust(expr))
    }
}
