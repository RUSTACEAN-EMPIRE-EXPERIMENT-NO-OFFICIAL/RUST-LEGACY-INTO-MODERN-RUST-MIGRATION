use reqwest::blocking::get;
use scraper::{Html, Selector};
use std::fs;
use std::path::Path;
use std::{env, io};

/// 공식 Rust 문서를 파싱하여 dynamic rules 생성
fn fetch_dynamic_rules() -> Vec<(String, String)> {
    let urls = vec![
        "https://doc.rust-lang.org/reference/",
        "https://doc.rust-lang.org/book/",
        "https://doc.rust-lang.org/edition-guide/",
        "https://rust-lang.github.io/api-guidelines/",
        "https://doc.rust-lang.org/unstable-book/",
        "https://doc.rust-lang.org/nightly/rustc/lints/listing/warn-by-default/deprecated.html",
    ];

    let mut rules = vec![];

    println!("[INFO] Fetching official Rust docs...");

    for url in urls {
        println!("  - {}", url);

        let Ok(resp) = get(url) else { continue };
        let Ok(text) = resp.text() else { continue };

        let doc = Html::parse_document(&text);
        let selector = Selector::parse("body").unwrap();
        let body_text = doc
            .select(&selector)
            .next()
            .map(|e| e.text().collect::<String>().to_lowercase())
            .unwrap_or_default();

        // Rule: prefer ? operator
        if body_text.contains("prefer the ? operator")
            || body_text.contains("use the ? operator")
            || body_text.contains("the ? operator")
        {
            rules.push((".unwrap()".into(), "?".into()));
            rules.push(("expect(".into(), "? /* expect */ (".into()));
        }

        // try! deprecated
        if body_text.contains("try! macro") && body_text.contains("deprecated") {
            rules.push(("try!(".into(), "?".into()));
        }

        // avoid println!
        if body_text.contains("avoid println") || body_text.contains("avoid printing") {
            rules.push(("println!".into(), "log::info!".into()));
        }

        // Deprecated APIs
        let deprecated_list = [
            "description()",
            "mem::uninitialized",
            "std::sync::once_init",
        ];

        for dep in deprecated_list {
            if body_text.contains(&dep.to_lowercase()) {
                rules.push((dep.into(), format!("/* deprecated: {} */", dep)));
            }
        }
    }

    println!("[INFO] Dynamic rules generated: {}", rules.len());
    rules
}

/// 문자열 기반 대체
fn apply_rules(code: &str, rules: &[(String, String)]) -> String {
    let mut new_code = code.to_string();
    for (old, new) in rules {
        new_code = new_code.replace(old, new);
    }
    new_code
}

/// Diff 출력
fn print_diff(old: &str, new: &str) {
    println!("--- DIFF START ----------------------");
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    for i in 0..old_lines.len().max(new_lines.len()) {
        let old_line = old_lines.get(i).unwrap_or(&"");
        let new_line = new_lines.get(i).unwrap_or(&"");

        if old_line != new_line {
            println!("- {}", old_line);
            println!("+ {}", new_line);
        }
    }
    println!("--- DIFF END ------------------------");
}

/// pause 지원 (크로스 플랫폼)
fn pause() {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let _ = Command::new("cmd").args(&["/C", "pause"]).status();
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::io::{self, Read};
        println!("Press ENTER to continue...");
        let _ = io::stdin().read(&mut [0u8]);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 3 {
        eprintln!("Usage:");
        eprintln!("  rust_modernizer <input.rs> <output.rs>");
        pause();
        return;
    }

    let input = &args[1];
    let output = &args[2];

    if !Path::new(input).exists() {
        eprintln!("❌ File not found: {}", input);
        pause();
        return;
    }

    println!("=== Rust Legacy → Modern Migration Tool ===");
    println!("Input  : {}", input);
    println!("Output : {}", output);

    let original = fs::read_to_string(input).expect("Failed to read input file");

    println!("--- Legacy Code Preview ---");
    println!("{}", original);
    println!("---------------------------");

    println!("⚙️ Generating dynamic rules from rust-lang.org ...");
    let rules = fetch_dynamic_rules();

    println!("⚙️ Applying rules...");
    let modernized = apply_rules(&original, &rules);

    print_diff(&original, &modernized);

    if let Some(parent) = Path::new(output).parent() {
        fs::create_dir_all(parent).ok();
    }

    fs::write(output, &modernized).expect("Failed to write output");

    println!("✅ modern_output.rs 생성 완료!");

    pause();
}


