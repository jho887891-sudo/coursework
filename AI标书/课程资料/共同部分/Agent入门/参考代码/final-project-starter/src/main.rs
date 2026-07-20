use final_project_starter::review;
use std::{env, fs, path::Path, process};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("usage: final-project-starter <bid.txt> [--rules <rules.txt|rules-dir>] [--output <report.json>]");
        eprintln!();
        eprintln!("  bid.txt   Path to bid document (one clause per line: c-XX text)");
        eprintln!("  --rules    Path to rules file or directory (default: built-in rules)");
        eprintln!("  --output   Path for JSON report output (default: report.json)");
        process::exit(2);
    }

    let bid_path = &args[1];

    // Parse optional flags
    let mut rules_path: Option<String> = None;
    let mut output_path = String::from("report.json");
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--rules" => {
                i += 1;
                if i < args.len() {
                    rules_path = Some(args[i].clone());
                } else {
                    eprintln!("error: --rules requires a path argument");
                    process::exit(2);
                }
            }
            "--output" => {
                i += 1;
                if i < args.len() {
                    output_path = args[i].clone();
                } else {
                    eprintln!("error: --output requires a path argument");
                    process::exit(2);
                }
            }
            other => {
                eprintln!("error: unknown flag: {}", other);
                process::exit(2);
            }
        }
        i += 1;
    }

    // Load bid document
    let bid_text = match fs::read_to_string(bid_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: cannot read bid file '{}': {}", bid_path, e);
            process::exit(1);
        }
    };

    // Load rules
    let rules_text = match &rules_path {
        Some(path) => {
            let p = Path::new(path);
            if p.is_dir() {
                // Load all .jsonl and .txt files from directory
                load_rules_from_dir(p)
            } else {
                match fs::read_to_string(p) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("error: cannot read rules file '{}': {}", path, e);
                        process::exit(1);
                    }
                }
            }
        }
        None => String::new(),
    };

    // Run review
    let report = match review(&bid_text, &rules_text) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: review failed: {:?}", e);
            process::exit(1);
        }
    };

    // Serialize to JSON
    let json = match serde_json::to_string_pretty(&report) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("error: JSON serialization failed: {}", e);
            process::exit(1);
        }
    };

    // Write report
    if let Err(e) = fs::write(&output_path, &json) {
        eprintln!("error: cannot write report to '{}': {}", output_path, e);
        process::exit(1);
    }

    // Print summary
    let risk_count = report
        .clauses
        .iter()
        .filter(|c| matches!(c.risk_decision, final_project_starter::RiskDecision::Risk))
        .count();
    let undetermined_count = report
        .clauses
        .iter()
        .filter(|c| matches!(c.risk_decision, final_project_starter::RiskDecision::Undetermined))
        .count();
    let no_risk_count = report.clauses.len() - risk_count - undetermined_count;

    println!("Document: {}", report.document_id);
    println!("Clauses: {} total", report.clauses.len());
    println!("  Risk:         {}", risk_count);
    println!("  No Risk:      {}", no_risk_count);
    println!("  Undetermined: {}", undetermined_count);
    println!("Report saved to: {}", output_path);
}

fn load_rules_from_dir(dir: &Path) -> String {
    let mut rules = String::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "jsonl" || e == "txt") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if !rules.is_empty() {
                        rules.push('\n');
                    }
                    // Parse JSONL if it's a .jsonl file
                    if path.extension().map_or(false, |e| e == "jsonl") {
                        for line in content.lines() {
                            let line = line.trim();
                            if line.is_empty() {
                                continue;
                            }
                            if let Ok(v) =
                                serde_json::from_str::<serde_json::Value>(line)
                            {
                                if let (Some(src_id), Some(loc), Some(text)) = (
                                    v["source_id"].as_str(),
                                    v["locator"].as_str(),
                                    v["verbatim_text"].as_str(),
                                ) {
                                    rules.push_str(&format!(
                                        "{}#{} {}\n",
                                        src_id, loc, text
                                    ));
                                }
                            }
                        }
                    } else {
                        rules.push_str(&content);
                    }
                }
            }
        }
    }
    rules
}
