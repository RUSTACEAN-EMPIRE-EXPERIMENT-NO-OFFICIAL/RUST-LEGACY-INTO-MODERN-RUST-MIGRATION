import os
import sys
import requests
from bs4 import BeautifulSoup

# ============================================================
# Optional Tree-sitter (Windows에서는 자동 비활성)
# ============================================================
ENABLE_AST = True
parser = None

try:
    from tree_sitter import Language, Parser

    TS_LIB = "tree-sitter-langs.so"
    if not os.path.exists(TS_LIB):
        print("[INFO] Building tree-sitter (may fail on Windows)...")
        Language.build_library(
            TS_LIB,
            ["https://github.com/tree-sitter/tree-sitter-rust"]
        )

    RUST = Language(TS_LIB, "rust")
    parser = Parser()
    parser.set_language(RUST)
    print("[INFO] Tree-sitter loaded successfully.")

except Exception as e:
    print(f"[WARN] Tree-sitter disabled (OS incompatible or build failed): {e}")
    ENABLE_AST = False


# ============================================================
# Official Rust Documentation (Dynamic rule generator)
# ============================================================
RUST_DOCS = [
    "https://doc.rust-lang.org/reference/",
    "https://doc.rust-lang.org/book/",
    "https://doc.rust-lang.org/edition-guide/",
    "https://rust-lang.github.io/api-guidelines/",
    "https://doc.rust-lang.org/unstable-book/",
    "https://doc.rust-lang.org/nightly/rustc/lints/listing/warn-by-default/deprecated.html",
]

def fetch_html(url):
    print(f"[INFO] Fetching: {url}")
    try:
        r = requests.get(url, timeout=10)
        if r.status_code == 200:
            return r.text
    except:
        pass
    return ""

def extract_rules():
    """
    Rust 공식 문서 기반의 dynamic rule generation (하드코딩 없음)
    """
    rules = {}

    for url in RUST_DOCS:
        html = fetch_html(url)
        if not html:
            continue

        soup = BeautifulSoup(html, "html.parser")
        text = soup.get_text().lower()

        # Prefer ? operator → unwrap/expect rewrite
        if "prefer the ? operator" in text or "use the ? operator" in text:
            rules["unwrap"] = (".unwrap()", "?")
            rules["expect"] = ("expect(", "? /* expect */")

        # try!() deprecated
        if "try! macro" in text and "deprecated" in text:
            rules["try_macro"] = ("try!(", "?")

        # Avoid println
        if "avoid println" in text or "avoid printing" in text:
            rules["println"] = ("println!", "log::info!")

        # Deprecated API set
        deprecated_candidates = [
            "description()",
            "mem::uninitialized",
            "std::sync::onCe_init".lower()
        ]
        for d in deprecated_candidates:
            if d.lower() in text:
                rules[d] = (d, f"/* deprecated: {d} */")

    print(f"[INFO] Dynamic rules generated: {len(rules)}")
    return rules


# ============================================================
# AST-based transform (Optional)
# ============================================================
def ast_transform(code):
    if not ENABLE_AST:
        return code

    try:
        tree = parser.parse(code.encode())
        root = tree.root_node
        if b"unsafe" in code:
            print("  - Found unsafe block")
        return code

    except Exception as e:
        print(f"[WARN] AST transform disabled: {e}")
        return code


# ============================================================
# Simple rewrite engine
# ============================================================
def apply_rules(code, rules):
    updated = code
    for _, (old, new) in rules.items():
        updated = updated.replace(old, new)
    return updated


def process_file(src, dst, rules):
    print(f"[FILE] {src} → {dst}")

    with open(src, "r", encoding="utf8") as f:
        code = f.read()

    code = ast_transform(code)
    code = apply_rules(code, rules)

    os.makedirs(os.path.dirname(dst), exist_ok=True)
    with open(dst, "w", encoding="utf8") as f:
        f.write(code)


# ============================================================
# Main entry (File or Directory)
# ============================================================
def main():
    if len(sys.argv) < 3:
        print("Usage:")
        print("  rust_modernizer.py <src_file> <dst_file>")
        print("  rust_modernizer.py <src_dir>  <dst_dir>")
        return

    src = sys.argv[1]
    dst = sys.argv[2]

    print("[INFO] Generating rules ...")
    rules = extract_rules()

    # FILE → FILE
    if os.path.isfile(src):
        process_file(src, dst, rules)
        print("[INFO] File modernize completed.")
        return

    # DIRECTORY → DIRECTORY
    print(f"[INFO] Directory rewrite: {src} → {dst}")
    for root, _, files in os.walk(src):
        for f in files:
            if f.endswith(".rs"):
                src_file = os.path.join(root, f)
                rel = os.path.relpath(src_file, src)
                dst_file = os.path.join(dst, rel)
                process_file(src_file, dst_file, rules)

    print("[INFO] Rust Modernizer completed.")


if __name__ == "__main__":
    main()


