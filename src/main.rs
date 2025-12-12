use anyhow::{Context, Result};
use clap::Parser;
use std::{collections::HashMap, fs, path::PathBuf};
use syn::{
    parse_quote,
    visit_mut::{self, VisitMut},
    Expr, ExprCall, ExprMethodCall, Lit,
};
use serde::{Deserialize, Serialize}; // serde 의존성 추가

/// ----------------------------------------------------
/// 0. 상수 및 규칙 모델 정의
/// ----------------------------------------------------
const DOC_URL_UNWRAP_TO_TRY: &str = "https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html#a-shortcut-for-propagating-errors-the--operator";
const DOC_URL_MEM_UNINITIALIZED: &str = "https://doc.rust-lang.org/std/mem/fn.uninitialized";

/// AST 변환을 위한 단일 규칙을 정의하는 구조체 (JSON에서 로드됨)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModernizerRule {
    /// 규칙 ID (보고서 및 로그용)
    id: String,
    /// 매칭할 AST 타입 (현재는 ExprMethodCall, ExprCall 지원)
    ast_type: String, 
    /// 매칭할 메서드 이름 (.unwrap, .expect, uninitialized 등)
    method_name: String, 
    /// 매칭할 인자 개수
    args_count: u8,
    /// 대체할 Rust 코드 템플릿 (parse_quote!에 사용됨)
    replacement_template: String,
    /// 로그에 사용할 경고/정보 수준 (예: "✅", "⚠️", "❌")
    level_icon: String,
    /// 공식 문서 URL
    doc_url: String,
    /// 특수 패턴 매칭을 위한 플래그 (예: ok().unwrap() 매칭 시 "ok")
    nested_method: Option<String>, 
}


/// ----------------------------------------------------
/// 1. CLI 구조 정의 (clap)
/// ----------------------------------------------------
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = "Rust Legacy Code Modernizer using AST traversal.")]
struct Args {
    /// 변환할 Rust 파일 경로
    input: PathBuf,

    /// 변환된 코드를 저장할 출력 파일 경로
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// 원본 파일을 직접 덮어쓰기
    #[arg(long, default_value_t = false)]
    inplace: bool,

    /// 실제 파일을 저장하지 않고 변환 결과만 터미널에 출력
    #[arg(long, default_value_t = false)]
    dry_run: bool,
    
    /// 규칙 파일을 지정합니다. (기본값: modernizer_rules.json)
    #[arg(long, default_value = "modernizer_rules.json")]
    rules_file: PathBuf,
}

/// ----------------------------------------------------
/// 2. AST 변환기 정의 (syn::VisitMut)
/// ----------------------------------------------------
struct Modernizer {
    changed: bool, 
    counters: HashMap<String, u32>, // 규칙 ID별 카운터
    rules: Vec<ModernizerRule>, 
}

impl Modernizer {
    fn new(rules: Vec<ModernizerRule>) -> Self {
        Modernizer {
            changed: false,
            counters: HashMap::new(),
            rules,
        }
    }
    
    /// 규칙 템플릿을 기반으로 AST 노드를 생성합니다.
    fn apply_rule_template(&self, method_call: &ExprMethodCall, rule: &ModernizerRule) -> Option<Expr> {
        let span = method_call.method.span();
        let receiver = method_call.receiver.clone();
        let method = method_call.method.clone();
        let doc_url = &rule.doc_url;

        // 경고/DOC 주석을 포함하는 템플릿 구조를 정의
        let template_with_doc = format!("// DOC: {} (Ref: {}) \n{}", 
            rule.id, doc_url, rule.replacement_template
        );

        match rule.id.as_str() {
            "unwrap_to_try" | "expect_to_try" => {
                // .unwrap()이나 .expect()의 경우: #receiver?
                // parse_quote!는 컴파일 타임 매크로이므로, 문자열 템플릿을 직접 삽입할 수 없습니다.
                // 따라서 ID별로 하드코딩된 parse_quote!를 유지하고, DOC 주석만 동적으로 삽입합니다.

                let note = if rule.id == "expect_to_try" {
                    // Expect 메시지 추출 로직 (복잡하므로 간소화)
                    let msg = "Expect message removed, manual review needed.";
                    format!("// NOTE: {}", msg)
                } else {
                    String::new()
                };

                Some(parse_quote! {
                    // DOC: Converted legacy call to `?` (idiomatic error propagation). Ref: #doc_url
                    #note
                    #receiver? 
                })
            }
            "ok_unwrap_to_try" => {
                // ok().unwrap()의 경우: inner_call.receiver?
                if let Expr::MethodCall(inner_call) = &*method_call.receiver {
                    let inner_receiver = inner_call.receiver.clone();
                    Some(parse_quote! {
                        // DOC: Converted `ok().unwrap()` to `?`. Ref: #doc_url
                        #inner_receiver? 
                    })
                } else {
                    None
                }
            }
            _ => {
                // 다른 일반적인 메서드 호출 처리
                // 여기서는 리시버가 없는 함수 호출(ExprCall)은 처리하지 않음
                None
            }
        }
    }
    
    /// 로드된 규칙을 순회하며 메서드 호출을 변환합니다.
    fn transform_method_call(&mut self, method_call: &ExprMethodCall) -> Option<Expr> {
        let method_name = method_call.method.to_string();
        
        for rule in &self.rules {
            if rule.ast_type != "ExprMethodCall" { continue; }

            // 1. 기본 매칭: 메서드 이름 및 인자 개수
            if rule.method_name == method_name && rule.args_count as usize == method_call.args.len() {
                
                // 2. 특수 패턴 (nested_method) 추가 매칭 검사
                let is_nested_match = match rule.nested_method.as_deref() {
                    Some(nested) => {
                        if let Expr::MethodCall(inner_call) = &*method_call.receiver {
                            inner_call.method.to_string() == nested
                        } else {
                            false
                        }
                    }
                    None => true,
                };

                if is_nested_match {
                    if let Some(new_expr) = self.apply_rule_template(method_call, rule) {
                        println!("[MOD] {} {} applied (Span: {:?})", rule.level_icon, rule.id, method_call.method.span());
                        self.changed = true;
                        *self.counters.entry(rule.id.clone()).or_insert(0) += 1;
                        return Some(new_expr);
                    }
                }
            }
        }
        None
    }
    
    /// 로드된 규칙을 순회하며 함수 호출을 변환합니다. (uninitialized 전용)
    fn transform_expr_call(&mut self, expr_call: &ExprCall) -> Option<Expr> {
        for rule in &self.rules {
            if rule.ast_type != "ExprCall" { continue; }
            
            // `uninitialized` 규칙에 대한 특수 로직
            if rule.id == "mem_uninitialized_to_maybeuninit" {
                if let Expr::Path(expr_path) = &*expr_call.func {
                    if let Some(segment) = expr_path.path.segments.last() {
                        if segment.ident.to_string() == rule.method_name && expr_call.args.is_empty() {
                            println!("[MOD] {} {} applied (Span: {:?})", rule.level_icon, rule.id, segment.ident.span());
                            self.changed = true;
                            *self.counters.entry(rule.id.clone()).or_insert(0) += 1;
                            
                            let doc_url = &rule.doc_url;
                            
                            // uninitialized 변환은 unsafe 코드가 필요하므로 하드코딩된 parse_quote를 사용
                            return Some(parse_quote! {
                                // DOC: `std::mem::uninitialized` is deprecated. Replaced with `MaybeUninit` usage.
                                // WARNING: This conversion remains `unsafe` and MUST be manually reviewed for initialization correctness.
                                // Ref: #doc_url
                                unsafe { 
                                    std::mem::MaybeUninit::uninit().assume_init()
                                }
                            });
                        }
                    }
                }
            }
        }
        None
    }
}

impl VisitMut for Modernizer {
    fn visit_expr_mut(&mut self, i: &mut Expr) {
        // 1. 깊이 우선 순회
        visit_mut::visit_expr_mut(self, i); 
        
        let new_expr = match i {
            // (1) 메서드 호출 변환 (데이터 기반)
            Expr::MethodCall(method_call) => self.transform_method_call(method_call),
            
            // (2) 함수 호출 변환 (데이터 기반)
            Expr::Call(expr_call) => self.transform_expr_call(expr_call),

            // (3) 기타 리터럴 패턴 확인 (이것은 데이터 기반으로 전환하기 복잡하여 유지)
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

fn load_rules(file_path: &PathBuf) -> Result<Vec<ModernizerRule>> {
    println!("📖 규칙 파일 로드 중: {}", file_path.display());
    
    let rule_json = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read rule file: {}", file_path.display()))?;
    
    let rules: Vec<ModernizerRule> = serde_json::from_str(&rule_json)
        .with_context(|| "Failed to parse modernizer_rules.json. Check JSON format.")?;
        
    Ok(rules)
}

fn main() -> Result<()> {
    // 1. CLI 인자 파싱
    let args = Args::parse();
    
    // 2. 규칙 로드
    let rules = load_rules(&args.rules_file)?;

    // 3. 출력 경로 결정
    let output_path = match &args.output {
        Some(path) => path.clone(),
        None if args.inplace => args.input.clone(),
        None => PathBuf::from("modernized_output.rs"),
    };
    
    // ... (CLI 출력 유지)
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

    // 4. 파일 읽기 및 AST 생성
    let source_code = fs::read_to_string(&args.input)
        .with_context(|| format!("Failed to read input file: {}", args.input.display()))?;
    
    let mut ast = syn::parse_file(&source_code)
        .with_context(|| format!("Failed to parse Rust code as AST: {}", args.input.display()))?;
    
    // 5. AST 변환 적용
    println!("\n⚙️ Modernizing code using AST traversal...");
    let mut modernizer = Modernizer::new(rules);
    modernizer.visit_file_mut(&mut ast); // AST의 루트 노드(File)부터 변환기 적용
    // 

    // 6. 변경 사항 확인 및 보고서 출력
    if !modernizer.changed {
        println!("\nℹ️ 코드 변경 사항이 감지되지 않았습니다.");
        return Ok(());
    }
    
    println!("\n📊 변환 보고서:");
    for (id, count) in modernizer.counters {
        // 규칙 ID를 기반으로 출력 (추가적인 상세 정보는 ModernizerRule에서 가져와야 함)
        println!("  - {} 건 ({})", count, id);
    }


    // 7. AST를 코드 문자열로 재구성 및 8. 파일 I/O
    let modernized_code = prettyplease::unparse(&ast); 

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
