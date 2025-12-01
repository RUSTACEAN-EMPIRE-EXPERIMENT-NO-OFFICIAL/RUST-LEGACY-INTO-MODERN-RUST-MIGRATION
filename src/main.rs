use std::io::{self, Write};
use std::process::Command;
use std::fs;
use similar::{TextDiff, ChangeTag};

fn pause() {
    let mut input = String::new();
    println!("\nPress ENTER to continue...");
    let _ = io::stdin().read_line(&mut input);
}

fn main() {
    println!("=== Rust Legacy → Modern Migration Tool ===\n");
    println!("변환할 Rust 파일 경로를 입력하세요.");
    print!("> ");
    io::stdout().flush().unwrap();

    let mut file_path = String::new();
    io::stdin().read_line(&mut file_path).unwrap();
    let file_path = file_path.trim();

    if !std::path::Path::new(file_path).exists() {
        println!("❌ 파일을 찾을 수 없습니다: {}", file_path);
        pause();
        return;
    }

    let legacy_code = fs::read_to_string(file_path).expect("파일 읽기 실패");
    println!("\n--- Legacy Code Preview ---\n{}\n---------------------------", legacy_code);

    println!("⚙️ Modernizer( Python ) 실행 중...");
    let output = Command::new("python3")
        .arg("tools/rust_modernizer.py")
        .arg(file_path)
        .arg("modern_output.rs")
        .output();

    let output = match output {
        Ok(o) => o,
        Err(_) => {
            println!("❌ Modernizer 실행 실패");
            pause();
            return;
        }
    };

    if !std::path::Path::new("modern_output.rs").exists() {
        println!("❌ modern_output.rs 생성 실패");
        pause();
        return;
    }

    let modern_code = fs::read_to_string("modern_output.rs").unwrap();

    println!("\n=== Diff (Legacy → Modern) ===\n");

    let diff = TextDiff::from_lines(&legacy_code, &modern_code);

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => print!("\x1b[31m-{}\x1b[0m", change),
            ChangeTag::Insert => print!("\x1b[32m+{}\x1b[0m", change),
            ChangeTag::Equal  => print!(" {}", change),
        }
    }

    println!("\n==============================\n");

    // Overwrite?
    println!("변환된 코드를 원본 파일에 덮어쓸까요? (y/N)");
    print!("> ");
    io::stdout().flush().unwrap();

    let mut overwrite = String::new();
    io::stdin().read_line(&mut overwrite).unwrap();

    if overwrite.trim().eq_ignore_ascii_case("y") {
        fs::write(file_path, modern_code).unwrap();
        println!("✔️ 파일 덮어쓰기 완료!");
    } else {
        println!("✔️ modern_output.rs 로 저장됨");
    }

    pause();
}
