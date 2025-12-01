use std::io::{self, Write};
use std::process::{Command, Output};
use std::fs;
use similar::{TextDiff, ChangeTag};

fn pause() {
    let mut input = String::new();
    println!("\nPress ENTER to continue...");
    let _ = io::stdin().read_line(&mut input);
}

fn run_python_modernizer(input: &str, output: &str) -> Option<Output> {
    // 1) python3 먼저 시도
    let try_py3 = Command::new("python3")
        .arg("tools/rust_modernizer.py")
        .arg(input)
        .arg(output)
        .output();

    if try_py3.is_ok() {
        return Some(try_py3.unwrap());
    }

    // 2) python이 있는 OS에서는 python 시도
    let try_py = Command::new("python")
        .arg("tools/rust_modernizer.py")
        .arg(input)
        .arg(output)
        .output();

    if try_py.is_ok() {
        return Some(try_py.unwrap());
    }

    None
}

fn print_diff(old: &str, new: &str) {
    println!("\n=== Diff (Legacy → Modern) ===\n");

    let diff = TextDiff::from_lines(old, new);

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => print!("\x1b[31m-{}\x1b[0m", change),
            ChangeTag::Insert => print!("\x1b[32m+{}\x1b[0m", change),
            ChangeTag::Equal  => print!(" {}", change),
        }
    }

    println!("\n==============================");
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

    println!("⚙️ Modernizer(Python) 실행 중...");

    let python_output = run_python_modernizer(file_path, "modern_output.rs");

    match python_output {
        None => {
            println!("❌ Python 실행 자체가 실패했습니다.");
            println!("python3 또는 python 명령이 없는 환경일 수 있습니다.");
            pause();
            return;
        }
        Some(out) => {
            println!("\n=== Modernizer STDOUT ===");
            println!("{}", String::from_utf8_lossy(&out.stdout));

            println!("\n=== Modernizer STDERR ===");
            println!("{}", String::from_utf8_lossy(&out.stderr));
        }
    }

    if !std::path::Path::new("modern_output.rs").exists() {
        println!("❌ modern_output.rs 생성 실패");
        println!("Modernizer 파이썬 로직 안에서 예외가 발생했을 가능성이 있습니다.");
        println!("위의 STDERR 출력을 검토하세요!");
        pause();
        return;
    }

    let modern_code = fs::read_to_string("modern_output.rs").unwrap();

    // Diff 출력
    print_diff(&legacy_code, &modern_code);

    // 파일 덮어쓰기 여부
    println!("\n변환된 코드를 원본 파일에 덮어쓸까요? (y/N)");
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
