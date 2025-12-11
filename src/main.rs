use anyhow::{Context, Result};
use clap::Parser;
use std::{fs, path::PathBuf};
use syn::{
    parse_quote,
    visit_mut::{self, VisitMut},
    Expr, ExprMethodCall, Lit,
};

/// ----------------------------------------------------
/// 1. CLI 구조 정의 (clap)
/// ----------------------------------------------------
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = "Rust Legacy Code Modernizer using AST traversal.")]
struct Args {
    /// 변환할 Rust 파일 경로
    input: PathBuf,

    /// 변환된 코드를 저장할 출력 파일 경로
    /// --inplace가 지정되면 이 인자는 무시됩니다.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// 원본 파일을 직접 덮어쓰기 (--output 지정 시 무시됨)
    #[arg(long, default_value_t = false)]
    inplace: bool,
}

/// ----------------------------------------------------
/// 2. AST 변환기 정의 (syn::VisitMut)
/// ----------------------------------------------------
/// 'Legacy' 코드를 'Modern' 코드로 변환하고 변경 여부를 추적하는 구조체
struct Modernizer {
    /// AST가 변경되었는지 여부를 추적하는 플래그
    changed: bool, 
}

impl Modernizer {
    /// .unwrap() 호출을 ? 연산자를 사용하는 Expr::Try 형태로 변환합니다.
    fn transform_unwrap_to_try(&mut self, method_call: &ExprMethodCall) -> Option<Expr> {
        // 메서드 이름이 unwrap()이고 인자가 없는 경우
        if method_call.method.to_string() == "unwrap" && method_call.args.is_empty() {
            // Span 정보는 디버깅에 유용합니다. (파일 경로와 라인 정보)
            println!("[MOD] Found .unwrap() at span: {:?}", method_call.method.span());
            
            // syn::parse_quote!를 사용하여 Reciever 뒤에 ?를 붙인 새로운 Expr::Try를 생성합니다.
            let new_expr = parse_quote! {
                #method_call.receiver?
            };
            
            self.changed = true; // 변경 플래그 설정
            Some(new_expr)
        } else {
            None
        }
    }
}

impl VisitMut for Modernizer {
    fn visit_expr_mut(&mut self, i: &mut Expr) {
        // 1. 깊이 우선 순회: 하위 노드를 먼저 방문하고 변환
        visit_mut::visit_expr_mut(self, i); 
        
        // 2. 패턴 매칭을 통해 Legacy 패턴을 찾습니다.
        match i {
            // (1) .unwrap() -> ? 변환 로직 적용
            Expr::MethodCall(method_call) => {
                if let Some(new_expr) = self.transform_unwrap_to_try(method_call) {
                    *i = new_expr;
                }
            }
            
            // (2) Deprecated 리터럴 문자열 주석 처리 예시
            Expr::Lit(expr_lit) => {
                if let Lit::Str(lit_str) = &expr_lit.lit {
                    if lit_str.value().contains("mem::uninitialized") {
                        println!("[MOD] Found deprecated string pattern in literal.");
                        self.changed = true;
                        // 여기에 주석 처리 등의 변환 로직 추가
                    }
                }
            }
            
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
    
    // 2. 출력 경로 결정
    let output_path = match &args.output {
        Some(path) => path.clone(),
        None if args.inplace => args.input.clone(),
        None => PathBuf::from("modernized_output.rs"),
    };

    println!("============================================");
    println!("    Rust Legacy → Modern Migration Tool");
    println!("============================================\n");
    println!("📄 입력 파일: {}", args.input.display());
    println!("📁 출력 파일: {}", output_path.display());


    // 3. 파일 읽기 및 에러 핸들링
    let source_code = fs::read_to_string(&args.input)
        .with_context(|| format!("Failed to read input file: {}", args.input.display()))?;

    // 4. 코드 파싱 (AST 생성)
    let mut ast = syn::parse_file(&source_code)
        .with_context(|| format!("Failed to parse Rust code as AST: {}", args.input.display()))?;
    

    // 5. AST 변환 적용
    println!("\n⚙️ Modernizing code using AST traversal...");
    let mut modernizer = Modernizer { changed: false };
    modernizer.visit_file_mut(&mut ast); // AST의 루트 노드(File)부터 변환기 적용

    // 6. 변경 사항 확인 및 출력
    if !modernizer.changed {
        println!("\nℹ️ 코드 변경 사항이 감지되지 않았습니다. 파일 쓰기를 건너뜜.");
        return Ok(());
    }

    // 7. AST를 코드 문자열로 재구성 (prettyplease 사용)
    let modernized_code = prettyplease::unparse(&ast); 

    // 8. 결과 파일 쓰기
    fs::write(&output_path, modernized_code)
        .with_context(|| format!("Failed to write output file: {}", output_path.display()))?;

    println!("\n✅ 변환 완료!");
    println!("→ {}", output_path.display());
    
    Ok(())
}
