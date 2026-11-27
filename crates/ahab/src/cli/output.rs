//! What commands print: readable text by default, the API's JSON with
//! `--json`. Errors go to standard error, as text or as Problem Details.

use serde_json::{Value, json};

use crate::cli::exit::Exit;

/// Prints a result: `value` as JSON with `--json`, else `text`.
pub fn print(json: bool, value: &Value, text: impl FnOnce() -> String) {
    if json {
        println!("{value}");
    } else {
        let text = text();
        println!("{}", text.trim_end());
    }
}

/// Prints an error from the API, in the Problem Details format.
pub fn error(json: bool, problem: &Value) {
    if json {
        eprintln!("{problem}");
        return;
    }
    let text = |key: &str| problem[key].as_str().filter(|text| !text.is_empty());
    let mut message = text("detail")
        .or(text("title"))
        .unwrap_or("The call failed.")
        .to_owned();
    if let Some(code) = text("code") {
        message.push_str(&format!(" ({code})"));
    }
    eprintln!("Error: {message}");
    for error in problem["errors"].as_array().into_iter().flatten() {
        let field = error["field"].as_str().unwrap_or_default();
        let detail = error["message"].as_str().unwrap_or_default();
        if field.is_empty() {
            eprintln!("  {detail}");
        } else {
            eprintln!("  {field}: {detail}");
        }
    }
}

/// Prints a failure of the CLI's own, and gives its exit code.
pub fn failure(json: bool, exit: Exit, title: &str, detail: &str) -> Exit {
    error(json, &json!({ "title": title, "detail": detail }));
    exit
}

/// Rows as aligned columns; the first row is the header.
pub fn table(rows: &[Vec<String>]) -> String {
    let widths: Vec<usize> = (0..rows.first().map_or(0, Vec::len))
        .map(|column| {
            rows.iter()
                .map(|row| row.get(column).map_or(0, |cell| cell.chars().count()))
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut out = String::new();
    for row in rows {
        let mut line = String::new();
        for (column, cell) in row.iter().enumerate() {
            if column + 1 == row.len() {
                line.push_str(cell);
            } else {
                let pad = widths[column].saturating_sub(cell.chars().count());
                line.push_str(cell);
                line.push_str(&" ".repeat(pad + 2));
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// A field of a JSON answer as text: empty when missing.
pub fn text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligns_columns() {
        let rows = vec![
            vec!["NAME".into(), "ROLE".into()],
            vec!["build-box".into(), "machine".into()],
        ];
        assert_eq!(table(&rows), "NAME       ROLE\nbuild-box  machine\n");
    }

    #[test]
    fn writes_fields_as_text() {
        assert_eq!(text(&json!("ok")), "ok");
        assert_eq!(text(&json!(3)), "3");
        assert_eq!(text(&Value::Null), "");
    }
}
