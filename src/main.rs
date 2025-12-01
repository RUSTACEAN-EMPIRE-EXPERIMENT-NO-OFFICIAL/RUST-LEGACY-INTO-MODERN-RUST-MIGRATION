use reqwest::blocking::get;
use scraper::{Html, Selector};
use std::fs;
use std::io::{self, Write};
use std::path::Path;

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

        // prefer ? operator
        if body_text.contains("prefer the ? operator")
            || body_text.contains("use the ? operator")
        {
            rules.push((".unwrap()".into(), "?".into()));
            rules.push(("expect(".into(), "? /* expect */ (".into()));
        }

        // try! deprecated
        if body_text.contains("try! macro") && body_text.contains("deprecated") {
            rules.push(("try!(".into(), "?".into()));
        }

        // avoid println!
        if body_text.contains("avoid println") {
            rules.push(("println!".into(), "log::info!".into()));
        }

        // deprecated API
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

fn apply_rules(code: &str, rules: &[(String, String)]) -> String {
    let mut new_code = code.to_string();
    for (old, new) in rules {
        new_code = new_code.replace(old, new);
    }
    new_code
}

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

/// pause (Windows 포함 모든 OS에서 작동)
fn pause() {
    let mut s = String::new();
    print!("\nPress ENTER to continue...");
    io::stdout().flush().unwrap();
    io::stdin().read_line(&mut s).unwrap();
}

fn main() {
    println!("============================================");
    println!("    Rust Legacy → Modern Migration Tool");
    println!("============================================\n");

    // 사용자 입력
    print!("변환할 Rust 파일 경로를 입력하세요.\n> ");
    io::stdout().flush().unwrap();

    let mut input_path = String::new();
    io::stdin().read_line(&mut input_path).unwrap();
    let input_path = input_path.trim();

    if !Path::new(input_path).exists() {
        eprintln!("❌ 파일이 존재하지 않습니다: {}", input_path);
        pause();
        return;
    }

    let output_path = "modern_output.rs";

    println!("\n--- Legacy Code Preview ---");
    let original = fs::read_to_string(input_path).expect("Failed to read file");
    println!("{}", original);
    println!("---------------------------");

    println!("⚙️ Rust 공식 문서 기반 Dynamic Rules 생성 중...");
    let rules = fetch_dynamic_rules();

    println!("⚙️ Modernizing code...");
    let modernized = apply_rules(&original, &rules);

    print_diff(&original, &modernized);

    fs::write(output_path, modernized).expect("Failed to write output");

    println!("✅ 변환 완료!");
    println!("→ 결과 파일: {}", output_path);

    pause();
}


