import os
import sys
import requests
from bs4 import BeautifulSoup
from tree_sitter import Language, Parser

TS_LIB = "tree-sitter-langs.so"

if not os.path.exists(TS_LIB):
    print("[INFO] Building tree-sitter...")
    Language.build_library(
        TS_LIB,
        ["https://github.com/tree-sitter/tree-sitter-rust"]
    )

RUST = Language(TS_LIB, "rust")
parser = Parser()
parser.set_language(RUST)

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
    rules = {}
    for url in RUST_DOCS:
        html = fetch_html(url)
        if not html:
            continue
        soup = BeautifulSoup(html, "html.parser")
        text = soup.get_text().lower()

        if "prefer the ? operator" in text or "use the ? operator" in text:
            rules["unwrap"] = (".unwrap()", "?")
            rules["expect"] = ("expect(", "? /* expect */")

        if "try! macro" in text and "deprecated" in text:
            rules["try_macro"] = ("try!(", "?")

        if "avoid println" in text or "avoid printing" in text:
            rules["println"] = ("println!", "log::info!")

        deprecated_candidates = [
            "description()",
            "mem::uninitialized",
            "std::sync::ONCE_INIT",
        ]
        for d in deprecated_candidates:
            if d.lower() in text:
                rules[d] = (d, f"/* deprecated: {d} */")

    print(f"[INFO] Dynamic rules generated: {len(rules)}")
    return rules

def ast_transform(code):
    tree = parser.parse(code.encode())
    root = tree.root_node
    if b"unsafe" in code:
        print("  - Found unsafe block")
    return code

def apply_rules(code, rules):
    updated = code
    for name, (old, new) in rules.items():
        updated = updated.replace(old, new)
    return updated

def process_file(src, dst, rules):
    print(f"[FILE] {src} → {dst}")
    code = open(src, "r", encoding="utf8").read()
    code = ast_transform(code)
    code = apply_rules(code, rules)

    os.makedirs(os.path.dirname(dst), exist_ok=True)
    with open(dst, "w", encoding="utf8") as f:
        f.write(code)

def main():
    if len(sys.argv) < 3:
        print("Usage:")
        print("  rust_modernizer.py <src_file> <dst_file>")
        print("  rust_modernizer.py <src_dir> <dst_dir>")
        return

    src = sys.argv[1]
    dst = sys.argv[2]

    print("[INFO] Generating dynamic rules from rust-lang.org ...")
    rules = extract_rules()

    # >>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>
    # FILE → FILE 모드
    # >>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>
    if os.path.isfile(src):
        process_file(src, dst, rules)
        print("[INFO] File modernize completed.")
        return

    # >>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>
    # DIRECTORY → DIRECTORY 모드
    # >>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>
    print(f"[INFO] Starting AST rewrite: {src} → {dst}")
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

