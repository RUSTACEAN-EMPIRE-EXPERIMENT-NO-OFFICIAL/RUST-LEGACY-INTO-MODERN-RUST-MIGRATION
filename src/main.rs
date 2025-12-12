use anyhow::{Context, Result};
use clap::Parser;
use std::{fs, path::PathBuf};
use syn::{
    parse_quote,
    visit_mut::{self, VisitMut},
    Expr, ExprCall, ExprMethodCall, Lit,
};

/// ----------------------------------------------------
/// 0. 상수: 공식 문서 참조 링크
/// ----------------------------------------------------
const DOC_URL_UNWRAP_TO_TRY: &str = "https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html#a-shortcut-for-propagating-errors-the--operator";
const DOC_URL_MEM_UNINITIALIZED: &str = "https://doc.rust-lang.org/std/mem/fn.uninitialized";

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

    /// 원본 파일을 직접 덮어쓰기
    #[arg(long, default_value_t = false)]
    inplace: bool,

    /// 실제 파일을 저장하지 않고 변환 결과만 터미널에 출력
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

/// ----------------------------------------------------
/// 2. AST 변환기 정의 (syn::VisitMut)
/// ----------------------------------------------------
/// 'Legacy' 코드를 'Modern' 코드로 변환하고 변경 여부 및 카운트를 추적하는 구조체
struct Modernizer {
    changed: bool, 
    unwrap_count: u32,
    expect_count: u32,
    ok_unwrap_count: u32, // `ok().unwrap()` 카운트
    uninitialized_count: u32, // `mem::uninitialized` 카운트
}

impl Modernizer {
    /// .unwrap(), .expect(), .ok().unwrap() 호출을 ? 연산자로 변환
    fn transform_method_call(&mut self, method_call: &ExprMethodCall) -> Option<Expr> {
        let method_name = method_call.method.to_string();
        let span = method_call.method.span(); 
        
        // 1. .unwrap() -> ? 변환
        if method_name == "unwrap" && method_call.args.is_empty() {
            
            // 1-1. `expr.ok().unwrap()` 패턴 확인
            if let Expr::MethodCall(inner_call) = &*method_call.receiver {
                if inner_call.method.to_string() == "ok" && inner_call.args.is_empty() {
                    println!("[MOD] ✅ `ok().unwrap()` -> `?` (Span: {:?})", span);
                    self.ok_unwrap_count += 1;
                    self.changed = true;
                    
                    // `(expr).ok().unwrap()`을 `(expr)?`로 변환하고 공식 문서 참조 주석 추가
                    return Some(parse_quote! {
                        // DOC: Converted `ok().unwrap()` (unsafe) to `?` (idiomatic error propagation).
                        // Ref: #DOC_URL_UNWRAP_TO_TRY
                        #inner_call.receiver?
                    });
                }
            }
            
            // 1-2. 일반적인 `expr.unwrap()` 패턴
            println!("[MOD] ✅ .unwrap() -> ? (Span: {:?})", span);
            self.unwrap_count += 1;
            self.changed = true;
            
            return Some(parse_quote! {
                // DOC: Converted `.unwrap()` (panic risk) to `?` (idiomatic error propagation).
                // Ref: #DOC_URL_UNWRAP_TO_TRY
                #method_call.receiver?
            });
            
        } 
        
        // 2. .expect("msg") -> ? 변환
        else if method_name == "expect" && method_call.args.len() == 1 {
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
            self.expect_count += 1;
            self.changed = true;
            
            return Some(parse_quote! {
                // DOC: Converted `.expect()` to `?`. Review if the original panic message should be kept
                // or if the function's error type needs adjustment for `?` to work correctly.
                // NOTE: Original .expect() message: #msg 
                // Ref: #DOC_URL_UNWRAP_TO_TRY
                #method_call.receiver?
            });
        } 
        
        None
    }

    /// `std::mem::uninitialized()` 호출을 `MaybeUninit`으로 변환
    fn transform_uninitialized(&mut self, expr_call: &ExprCall) -> Option<Expr> {
        if let Expr::Path(expr_path) = &*expr_call.func {
            if let Some(segment) = expr_path.path.segments.last() {
                // 경로의 마지막 세그먼트가 `uninitialized`인지 확인
                if segment.ident.to_string() == "uninitialized" {
                    println!("[MOD] ❌ Found deprecated `uninitialized` (Span: {:?}). Converted to `MaybeUninit`.", segment.ident.span());
                    self.uninitialized_count += 1;
                    self.changed = true;
                    
                    // `MaybeUninit::uninit().assume_init()`로 변환하고 경고 주석 추가
                    return Some(parse_quote! {
                        // DOC: `std::mem::uninitialized` is deprecated. Replaced with `MaybeUninit` usage.
                        // WARNING: This conversion remains `unsafe` and MUST be manually reviewed for initialization correctness.
                        // Ref: #DOC_URL_MEM_UNINITIALIZED
                        unsafe { 
                            std::mem::MaybeUninit::uninit().assume_init()
                        }
                    });
                }
            }
        }
        None
    }
}

impl VisitMut for Modernizer {
    fn visit_expr_mut(&mut self, i: &mut Expr) {
        // 1. 깊이 우선 순회: 하위 노드를 먼저 방문하고 변환
        visit_mut::visit_expr_mut(self, i); 
        
        // 2. 패턴 매칭을 통해 Legacy 패턴을 찾습니다.
        let new_expr = match i {
            // (1) .unwrap(), .expect(), .ok().unwrap() 변환
            Expr::MethodCall(method_call) => self.transform_method_call(method_call),
            
            // (2) `std::mem::uninitialized()` 함수 호출 변환
            Expr::Call(expr_call) => self.transform_uninitialized(expr_call),

            // (3) Deprecated 리터럴 문자열 주석 처리 예시 (변환 없음, 로그만)
            Expr::Lit(expr_lit) => {
                if let Lit::Str(lit_str) = &expr_lit.lit {
                    if lit_str.value().contains("mem::uninitialized") {
                        println!("[MOD] ℹ️ Found deprecated string pattern in literal.");
                        self.changed = true;
                    }
                }
                None
            }
            
            _ => None
        };

        if let Some(expr) = new_expr {
            *i = expr;
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
        ok_unwrap_count: 0,
        uninitialized_count: 0,
    };
    modernizer.visit_file_mut(&mut ast); // AST의 루트 노드(File)부터 변환기 적용

    // 6. 변경 사항 확인 및 보고서 출력
    if !modernizer.changed {
        println!("\nℹ️ 코드 변경 사항이 감지되지 않았습니다.");
        return Ok(());
    }
    
    // 변환 보고서
    println!("\n📊 변환 보고서:");
    println!("  - ✅ .unwrap() 변환 완료: {} 건", modernizer.unwrap_count);
    println!("  - ✅ .ok().unwrap() 변환 완료: {} 건", modernizer.ok_unwrap_count);
    println!("  - ⚠️ .expect() 변환 완료: {} 건 (수동 검토 필요)", modernizer.expect_count);
    println!("  - ❌ `mem::uninitialized` 변환: {} 건 (unsafe 코드, **필수 검토**)", modernizer.uninitialized_count);


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
