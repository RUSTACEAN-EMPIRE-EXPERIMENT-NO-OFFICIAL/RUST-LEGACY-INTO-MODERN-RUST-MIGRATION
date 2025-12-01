use reqwest::blocking::get;
use scraper::{Html, Selector};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

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

        // deprecated API checks
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

/// 문자열 치환으로 규칙 적용
fn apply_rules(code: &str, rules: &[(String, String)]) -> String {
    let mut new_code = code.to_string();

    for (old, new) in rules {
        new_code = new_code.replace(old, new);
    }

    new_code
}

/// 간단 diff 출력
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

/// pause 기능 (Windows / Mac / Linux 모두 작동)
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

    print!("변환할 Rust 파일 경로를 입력하세요.\n> ");
    io::stdout().flush().unwrap();

    let mut input_path_str = String::new();
    io::stdin().read_line(&mut input_path_str).unwrap();
    let input_path_str = input_path_str.trim();

    let input_path = Path::new(input_path_str);

    if !input_path.exists() {
        eprintln!("❌ 파일이 존재하지 않습니다: {}", input_path_str);
        pause();
        return;
    }

    let parent_dir: PathBuf = input_path
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();

    let output_path = parent_dir.join("modern_output.rs");

    println!("📄 입력 파일: {}", input_path.display());
    println!("📁 출력 파일: {}\n", output_path.display());

    // 원본 읽기
    let original =
        fs::read_to_string(input_path).expect("Failed to read input file");

    println!("--- Legacy Code Preview ---");
    println!("{}", original);
    println!("---------------------------\n");

    println!("⚙️ Rust 공식 문서 기반 Dynamic Rules 생성 중...");
    let rules = fetch_dynamic_rules();

    println!("⚙️ Modernizing code...");
    let modernized = apply_rules(&original, &rules);

    print_diff(&original, &modernized);

    // 출력 경로 생성
    fs::create_dir_all(&parent_dir).ok();
    fs::write(&output_path, modernized).expect("Failed to write output");

    println!("\n✅ 변환 완료!");
    println!("→ {}", output_path.display());

    pause();
}


