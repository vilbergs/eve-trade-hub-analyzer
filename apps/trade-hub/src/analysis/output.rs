//! Output adapters for the report binary. One `Renderable` impl per
//! row type, three formats (table / csv / json).

use std::io::Write;

use comfy_table::{Cell, ContentArrangement, Table};
use serde::Serialize;

#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    Table,
    Csv,
    Json,
}

pub trait Renderable: Serialize {
    /// Column headers for table + CSV output.
    fn headers() -> Vec<&'static str>;
    /// One row of cells (strings) in the same order as `headers`.
    fn cells(&self) -> Vec<String>;
}

pub fn render<T: Renderable>(
    rows: &[T],
    format: Format,
    w: &mut impl Write,
) -> std::io::Result<()> {
    match format {
        Format::Json => {
            serde_json::to_writer_pretty(&mut *w, rows)?;
            writeln!(w)?;
        }
        Format::Csv => {
            writeln!(w, "{}", T::headers().join(","))?;
            for r in rows {
                let cells: Vec<String> = r.cells().into_iter().map(csv_escape).collect();
                writeln!(w, "{}", cells.join(","))?;
            }
        }
        Format::Table => {
            let mut table = Table::new();
            table
                .set_content_arrangement(ContentArrangement::Dynamic)
                .set_header(T::headers().iter().map(|h| Cell::new(*h)));
            for r in rows {
                table.add_row(r.cells().into_iter().map(Cell::new));
            }
            writeln!(w, "{table}")?;
        }
    }
    Ok(())
}

fn csv_escape(s: String) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

/// Format an Option<f64> as a number with at most 2 decimals, "-" if None.
pub fn fmt_money(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.2}")).unwrap_or_else(|| "-".into())
}

pub fn fmt_pct(v: Option<f64>) -> String {
    v.map(|x| format!("{:.1}%", x * 100.0))
        .unwrap_or_else(|| "-".into())
}

pub fn fmt_int(v: i64) -> String {
    v.to_string()
}

pub fn fmt_opt_int(v: Option<i64>) -> String {
    v.map(|n| n.to_string()).unwrap_or_else(|| "-".into())
}

pub fn fmt_opt_f64(v: Option<f64>, decimals: usize) -> String {
    v.map(|n| format!("{n:.*}", decimals))
        .unwrap_or_else(|| "-".into())
}
