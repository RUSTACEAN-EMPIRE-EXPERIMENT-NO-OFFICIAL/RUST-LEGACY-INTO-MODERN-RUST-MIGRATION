use anyhow::{Context, Result};
use clap::Parser;
use std::{fs, path::PathBuf};
use syn::{
    visit_mut::{self, VisitMut}, // AST 순회를 위한 트레이트
    Expr, Lit,
};

/// ----------------------------------------------------
/// 1. CLI 구조 정의 (clap)
/// ----------------------------------------------------
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 변환할 Rust 파일 경로
    input: PathBuf,

    /// 변환된 코드를 저장할 출력 파일 경로 (지정하지 않으면 인플레이스)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// 원본 파일을 직접 덮어쓰기 (output이 지정되지 않은 경우에만 사용)
    #[arg(long, default_value_t = false)]
    inplace: bool,
}

/// ----------------------------------------------------
/// 2. AST 변환기 정의 (syn::VisitMut)
/// ----------------------------------------------------
/// 'Legacy' 코드를 'Modern' 코드로 변환하는 로직을 담은 구조체
struct Modernizer;

impl VisitMut for Modernizer {
    // 모든 AST 노드(여기서는 표현식, Expr)를 순회하며 방문(visit)할 수 있음.

    fn visit_expr_mut(&mut self, i: &mut Expr) {
        // 먼저 하위 노드를 방문하여 깊숙한 곳부터 변환
        visit_mut::visit_expr_mut(self, i); 
        
        // Match를 사용하여 특정 Legacy 패턴을 찾습니다.
        match i {
            // (1) .unwrap() -> ? 변환 로직 (간단화)
            // 실제 구현에서는 .unwrap() 앞의 코드 구조를 확인하는 복잡한 로직 필요
            Expr::MethodCall(method_call) => {
                // 메서드 이름이 unwrap()이고 인자가 없는 경우를 가정
                if method_call.method.to_string() == "unwrap" && method_call.args.is_empty() {
                    println!("[MOD] Found .unwrap() at {:?}", method_call.method.span());
                    
                    // .unwrap()을 ?로 안전하게 치환하는 것은 복잡하므로, 
                    // 여기서는 임시로 .expect("FIXME: unwrap")으로 변경 예시를 보여줍니다.
                    // 실제로는 syn::Expr::Try 형태로 변환해야 합니다.
                    *i = syn::parse_quote! { 
                        #method_call.receiver.expect("FIXME: unwrap should be '?'")
                    };
                }
            }
            
            // (2) Deprecated 리터럴 문자열 주석 처리 예시
            // 실제 Deprecated API 이름이나 버전 번호를 포함한 문자열을 찾습니다.
            Expr::Lit(expr_lit) => {
                if let Lit::Str(lit_str) = &expr_lit.lit {
                    if lit_str.value().contains("mem::uninitialized") {
                        println!("[MOD] Found deprecated string pattern.");
                        // 변환 로직...
                    }
                }
            }
            
            // 다른 Legacy 패턴 처리...
            _ => {}
        }
    }
}

/// ----------------------------------------------------
/// 3. 메인 함수 및 파일 I/O
/// ----------------------------------------------------
fn main() -> Result<()> {
    // 1. CLI 인자 파싱
    let args = Args::parse();
    
    let output_path = match &args.output {
        Some(path) => path.clone(),
        None if args.inplace => args.input.clone(),
        None => PathBuf::from("modernized_output.rs"), // 기본 출력 경로
    };

    println!("============================================");
    println!("    Rust Legacy → Modern Migration Tool");
    println!("============================================\n");
    println!("📄 입력 파일: {}", args.input.display());
    println!("📁 출력 파일: {}\n", output_path.display());


    // 2. 파일 읽기 (anyhow로 에러 처리 개선)
    let source_code = fs::read_to_string(&args.input)
        .with_context(|| format!("Failed to read input file: {}", args.input.display()))?;

    // 3. 코드 파싱 (syn::parse_file)
    let mut ast = syn::parse_file(&source_code)
        .with_context(|| format!("Failed to parse Rust code as AST: {}", args.input.display()))?;
    

    // 4. AST 변환 적용
    println!("⚙️ Modernizing code using AST traversal...");
    let mut modernizer = Modernizer;
    modernizer.visit_file_mut(&mut ast); // AST의 루트 노드(File)부터 변환기 적용

    // 5. AST를 코드 문자열로 재구성 (pretty-print)
    let modernized_code = prettyplease::unparse(&ast); // (prettyplease 크레이트가 필요할 수 있음)
    // 여기서는 syn::parse_quote!에 의존하므로, simple to_string()을 사용한다고 가정

    // 6. 결과 파일 쓰기
    fs::write(&output_path, modernized_code)
        .with_context(|| format!("Failed to write output file: {}", output_path.display()))?;

    println!("\n✅ 변환 완료!");
    println!("→ {}", output_path.display());
    
    Ok(()) // main 함수가 Result를 반환하도록 변경 (에러 핸들링)
}
