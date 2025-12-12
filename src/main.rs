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
    /// --inplace 또는 --dry-run이 지정되면 이 인자는 무시됩니다.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// 원본 파일을 직접 덮어쓰기 (--output 또는 --dry-run 지정 시 무시됨)
    #[arg(long, default_value_t = false)]
    inplace: bool,

    /// 실제 파일을 저장하지 않고 변환 결과만 터미널에 출력
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

/// ----------------------------------------------------
/// 2. AST 변환기 정의 (syn::VisitMut)
/// ----------------------------------------------------
/// 'Legacy' 코드를 'Modern' 코드로 변환하고 변경 여부를 추적하는 구조체
struct Modernizer {
    /// AST가 변경되었는지 여부를 추적하는 플래그
    changed: bool, 
    /// .unwrap() 변환 카운트
    unwrap_count: u32,
    /// .expect() 변환 카운트
    expect_count: u32,
}

impl Modernizer {
    /// .unwrap() 또는 .expect() 호출을 ? 연산자를 사용하는 Expr::Try 형태로 변환합니다.
    fn transform_unwrap_to_try(&mut self, method_call: &ExprMethodCall) -> Option<Expr> {
        let method_name = method_call.method.to_string();
        let span = method_call.method.span(); // 위치 정보 (라인/컬럼)

        if method_name == "unwrap" && method_call.args.is_empty() {
            // .unwrap() -> ? 변환
            println!("[MOD] ✅ .unwrap() -> ? (Span: {:?})", span);
            self.changed = true;
            self.unwrap_count += 1;
            
            // Reciever 뒤에 ?를 붙인 새로운 Expr::Try를 생성합니다.
            Some(parse_quote! {
                #method_call.receiver?
            })
        } else if method_name == "expect" && method_call.args.len() == 1 {
            // .expect("msg") -> ? 변환 및 경고 주석 추가
            
            let msg = if let Expr::Lit(expr_lit) = &method_call.args[0] {
                if let Lit::Str(lit_str) = &expr_lit.lit {
                    lit_str.value()
                } else {
                    String::from("<non-string-literal>")
                }
            } else {
                String::from("<complex-expression>")
            };

            println!("[MOD] ⚠️ .expect(\"{}\") -> ? (Span: {:?}, Manual review needed.)", msg, span);
            self.changed = true;
            self.expect_count += 1;
            
            // Receiver 뒤에 ?를 붙이고, expect 메시지는 주석으로 남깁니다.
            Some(parse_quote! {
                // NOTE: Original .expect() message: #msg 
                #method_call.receiver?
            })
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
            // (1) .unwrap() 및 .expect() 변환 로직 적용
            Expr::MethodCall(method_call) => {
                if let Some(new_expr) = self.transform_unwrap_to_try(method_call) {
                    *i = new_expr;
                }
            }
            
            // (2) Deprecated 리터럴 문자열 주석 처리 예시 (원본 유지)
            Expr::Lit(expr_lit) => {
                if let Lit::Str(lit_str) = &expr_lit.lit {
                    if lit_str.value().contains("mem::uninitialized") {
                        println!("[MOD] ℹ️ Found deprecated string pattern (mem::uninitialized).");
                        self.changed = true;
                        // 여기에 변환 로직을 추가하여 주석 처리 등을 할 수 있습니다.
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
    
    // Dry Run 모드 메시지
    if args.dry_run {
        println!("\n🚨 DRY-RUN MODE: 파일 쓰기 작업을 건너뜁니다.");
    }

    println!("============================================");
    println!("    Rust Legacy → Modern Migration Tool");
    println!("============================================\n");
    println!("📄 입력 파일: {}", args.input.display());
    
    if !args.dry_run {
        println!("📁 출력 파일: {}", output_path.display());
    }


    // 3. 파일 읽기 및 에러 핸들링
    let source_code = fs::read_to_string(&args.input)
        .with_context(|| format!("Failed to read input file: {}", args.input.display()))?;

    // 4. 코드 파싱 (AST 생성)
    let mut ast = syn::parse_file(&source_code)
        .with_context(|| format!("Failed to parse Rust code as AST: {}", args.input.display()))?;
    

    // 5. AST 변환 적용
    println!("\n⚙️ Modernizing code using AST traversal...");
    let mut modernizer = Modernizer { 
        changed: false, 
        unwrap_count: 0,
        expect_count: 0,
    };
    // 
    modernizer.visit_file_mut(&mut ast); // AST의 루트 노드(File)부터 변환기 적용

    // 6. 변경 사항 확인 및 보고서 출력
    if !modernizer.changed {
        println!("\nℹ️ 코드 변경 사항이 감지되지 않았습니다.");
        return Ok(());
    }
    
    // 변환 보고서
    println!("\n📊 변환 보고서:");
    println!("  - ✅ .unwrap() 변환 완료: {} 건", modernizer.unwrap_count);
    println!("  - ⚠️ .expect() 변환 완료: {} 건 (수동 검토 필요)", modernizer.expect_count);


    // 7. AST를 코드 문자열로 재구성 (prettyplease 사용)
    let modernized_code = prettyplease::unparse(&ast); 

    // 8. 결과 파일 쓰기 또는 Dry Run 출력
    if args.dry_run {
        println!("\n📄 Dry Run 결과 코드 (파일 저장 안 함):");
        println!("--------------------------------------------");
        println!("{}", modernized_code);
        println!("--------------------------------------------");
    } else {
        fs::write(&output_path, modernized_code)
            .with_context(|| format!("Failed to write output file: {}", output_path.display()))?;

        println!("\n✅ 변환 완료! 파일 저장됨.");
        println!("→ {}", output_path.display());
    }
    
    Ok(())
}
